//! Audio playback for TTS (ADR-0005) — a `cpal` **output** stream fed a small
//! PCM queue so `stop()` (barge-in) goes silent within one buffer (~10–20 ms).
//!
//! Rust owns the stream (Article I). Like capture, the `cpal::Stream` is `!Send`
//! on some backends, so it lives on a parked thread; the queue is shared. TTS
//! synthesises at [`TTS_RATE`]; playback resamples to the device rate.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::ipc::{AppError, AppResult};
use crate::voice::resample::Resampler16;

/// The rate Chatterbox synthesises at.
pub const TTS_RATE: u32 = 24_000;

/// One selectable output device.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct OutputDevice {
    /// Human-readable name; the selection key persisted in config.
    pub name: String,
    /// Whether this is the host default.
    pub is_default: bool,
}

/// Enumerate output devices ( empty, not an error, when there are none).
#[must_use]
pub fn list_output_devices() -> Vec<OutputDevice> {
    let host = cpal::default_host();
    let default_name = host
        .default_output_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|d| d.name().ok())
        .map(|name| OutputDevice {
            is_default: name == default_name,
            name,
        })
        .collect()
}

type Queue = Arc<Mutex<VecDeque<f32>>>;

/// A running output stream. Dropping it stops playback and joins the thread.
pub struct Playback {
    queue: Queue,
    resampler: Mutex<Resampler16>,
    device_rate: u32,
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Playback {
    /// Open `device_name` (or the default when `None`) and start an output
    /// stream that plays whatever is [`enqueue`](Self::enqueue)d.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] when there is no matching output device
    /// or the stream cannot be built.
    pub fn open(device_name: Option<&str>) -> AppResult<Self> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(name) => host
                .output_devices()
                .map_err(|e| {
                    AppError::BackendUnavailable(format!("enumerate output devices: {e}"))
                })?
                .find(|d| d.name().is_ok_and(|n| n == name))
                .ok_or_else(|| {
                    AppError::BackendUnavailable(format!("output device {name:?} not found"))
                })?,
            None => host.default_output_device().ok_or_else(|| {
                AppError::BackendUnavailable("no default output device".to_owned())
            })?,
        };
        let config = device
            .default_output_config()
            .map_err(|e| AppError::BackendUnavailable(format!("output config: {e}")))?;
        let device_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
        let queue_cb = Arc::clone(&queue);

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<AppResult<()>>();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

        let thread = std::thread::Builder::new()
            .name("voice-playback".into())
            .spawn(move || {
                let stream = match build_stream(&device, &config, channels, &queue_cb) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = ready_tx.send(Err(AppError::BackendUnavailable(format!(
                        "start output stream: {e}"
                    ))));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                let _ = stop_rx.recv();
                drop(stream);
            })
            .map_err(|e| AppError::internal("spawn playback thread", e))?;

        ready_rx
            .recv()
            .map_err(|_| AppError::BackendUnavailable("playback thread died".to_owned()))??;

        Ok(Self {
            queue,
            resampler: Mutex::new(Resampler16::new(TTS_RATE, device_rate)),
            device_rate,
            stop: Some(stop_tx),
            thread: Some(thread),
        })
    }

    /// Queue mono samples synthesised at [`TTS_RATE`] for playback.
    pub fn enqueue(&self, samples_24k: &[f32]) {
        let resampled = {
            let mut r = self
                .resampler
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            r.push(samples_24k)
        };
        if let Ok(mut q) = self.queue.lock() {
            q.extend(resampled);
        }
    }

    /// Drop everything queued — playback goes silent within one output buffer.
    pub fn stop(&self) {
        if let Ok(mut q) = self.queue.lock() {
            q.clear();
        }
        // Reset the resampler so a later `enqueue` starts clean.
        *self
            .resampler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Resampler16::new(TTS_RATE, self.device_rate);
    }

    /// Approximate milliseconds of audio still queued.
    #[must_use]
    pub fn queued_ms(&self) -> u32 {
        let n = self.queue.lock().map_or(0, |q| q.len());
        ((n as f64 / f64::from(self.device_rate)) * 1000.0) as u32
    }

    /// Nothing left to play (within ~one output buffer).
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.queue.lock().map_or(true, |q| q.is_empty())
    }
}

impl Drop for Playback {
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
    channels: usize,
    queue: &Queue,
) -> AppResult<cpal::Stream> {
    let err_fn = |e| tracing::warn!(target: "voice", "playback stream error: {e}");
    let cfg: cpal::StreamConfig = config.config();

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let queue = Arc::clone(queue);
            device.build_output_stream(
                &cfg,
                move |data: &mut [f32], _| {
                    let mut q = queue
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    for frame in data.chunks_mut(channels) {
                        let s = q.pop_front().unwrap_or(0.0);
                        for c in frame.iter_mut() {
                            *c = s;
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let queue = Arc::clone(queue);
            device.build_output_stream(
                &cfg,
                move |data: &mut [i16], _| {
                    let mut q = queue
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    for frame in data.chunks_mut(channels) {
                        let s = q.pop_front().unwrap_or(0.0);
                        let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                        for c in frame.iter_mut() {
                            *c = v;
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        other => {
            return Err(AppError::BackendUnavailable(format!(
                "unsupported output sample format {other:?}"
            )))
        }
    }
    .map_err(|e| AppError::BackendUnavailable(format!("build output stream: {e}")))?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_does_not_panic() {
        let list = list_output_devices();
        assert!(list.iter().filter(|d| d.is_default).count() <= 1);
        eprintln!("{} output device(s)", list.len());
    }

    #[test]
    fn queue_and_stop() {
        let Ok(pb) = Playback::open(None) else {
            eprintln!("no output device — skipping");
            return;
        };
        assert!(pb.is_idle());
        // ~0.5 s of 24 kHz audio.
        let samples: Vec<f32> = (0..12_000).map(|i| (i as f32 * 0.02).sin() * 0.1).collect();
        pb.enqueue(&samples);
        assert!(pb.queued_ms() > 200, "queued {} ms", pb.queued_ms());
        pb.stop();
        assert!(pb.is_idle());
        assert_eq!(pb.queued_ms(), 0);
    }
}
