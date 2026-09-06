//! Microphone capture via `cpal` (WASAPI on Windows), ADR-0005.
//!
//! Rust owns device selection and the capture stream (Article I). The `cpal`
//! `Stream` is `!Send` on some backends, so it lives on a dedicated thread that
//! parks until dropped; audio frames flow out over a `tokio` channel as
//! interleaved f32 at the device's native rate (the caller resamples to
//! [`super::SAMPLE_RATE`] with [`super::resample::ToMono16k`]).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::mpsc;

use crate::ipc::{AppError, AppResult};

/// One selectable input device.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct InputDevice {
    /// Human-readable name; also the selection key persisted in config.
    pub name: String,
    /// Whether this is the host's current default.
    pub is_default: bool,
}

/// Enumerate input devices. An empty list is returned (not an error) when the
/// machine has no microphone.
#[must_use]
pub fn list_input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|d| d.name().ok())
        .map(|name| InputDevice {
            is_default: name == default_name,
            name,
        })
        .collect()
}

/// A running capture. Dropping it stops the stream and joins the thread.
pub struct CaptureStream {
    sample_rate: u32,
    channels: u16,
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CaptureStream {
    /// Open `device_name` (or the default when `None`) and start streaming
    /// interleaved f32 frames to `frames`.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] when there is no matching input device or
    /// the stream cannot be built.
    pub fn open(
        device_name: Option<&str>,
        frames: mpsc::UnboundedSender<Vec<f32>>,
    ) -> AppResult<Self> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(name) => host
                .input_devices()
                .map_err(|e| AppError::BackendUnavailable(format!("enumerate input devices: {e}")))?
                .find(|d| d.name().is_ok_and(|n| n == name))
                .ok_or_else(|| {
                    AppError::BackendUnavailable(format!("input device {name:?} not found"))
                })?,
            None => host.default_input_device().ok_or_else(|| {
                AppError::BackendUnavailable("no default input device".to_owned())
            })?,
        };
        let config = device
            .default_input_config()
            .map_err(|e| AppError::BackendUnavailable(format!("input config: {e}")))?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<AppResult<()>>();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

        let thread = std::thread::Builder::new()
            .name("voice-capture".into())
            .spawn(move || {
                let stream = match build_stream(&device, &config, channels, frames) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = ready_tx.send(Err(AppError::BackendUnavailable(format!(
                        "start input stream: {e}"
                    ))));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                // Hold the stream alive until asked to stop.
                let _ = stop_rx.recv();
                drop(stream);
            })
            .map_err(|e| AppError::internal("spawn capture thread", e))?;

        ready_rx
            .recv()
            .map_err(|_| AppError::BackendUnavailable("capture thread died".to_owned()))??;

        Ok(Self {
            sample_rate,
            channels,
            stop: Some(stop_tx),
            thread: Some(thread),
        })
    }

    /// The device's native sample rate (Hz).
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The device's channel count (frames are interleaved).
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

impl Drop for CaptureStream {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    channels: u16,
    frames: mpsc::UnboundedSender<Vec<f32>>,
    // returns a live (playing after `.play()`) stream
) -> AppResult<cpal::Stream> {
    let err_fn = |e| tracing::warn!(target: "voice", "capture stream error: {e}");
    let cfg: cpal::StreamConfig = config.config();
    let f2 = frames.clone();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &cfg,
            move |data: &[f32], _| {
                let _ = frames.send(data.to_vec());
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &cfg,
            move |data: &[i16], _| {
                let _ = f2.send(
                    data.iter()
                        .map(|s| f32::from(*s) / f32::from(i16::MAX))
                        .collect(),
                );
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &cfg,
            move |data: &[u16], _| {
                let _ = f2.send(
                    data.iter()
                        .map(|s| (f32::from(*s) - 32768.0) / 32768.0)
                        .collect(),
                );
            },
            err_fn,
            None,
        ),
        other => {
            return Err(AppError::BackendUnavailable(format!(
                "unsupported sample format {other:?}"
            )))
        }
    }
    .map_err(|e| AppError::BackendUnavailable(format!("build input stream: {e}")))?;
    let _ = channels;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_does_not_panic() {
        let list = list_input_devices();
        // At most one default.
        assert!(list.iter().filter(|d| d.is_default).count() <= 1);
        eprintln!("{} input device(s)", list.len());
    }
}
