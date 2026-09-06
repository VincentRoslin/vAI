//! Audio rate conversion (ADR-0005):
//! - [`ToMono16k`] — device capture → 16 kHz mono (Silero VAD + faster-whisper).
//! - [`Resampler16`] — a generic mono `in_rate → out_rate` streaming resampler
//!   (Phase 19 playback: Chatterbox's 24 kHz → the output device rate).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::voice::SAMPLE_RATE;

/// Streaming converter: feed interleaved device frames, get 16 kHz mono samples.
pub struct ToMono16k {
    channels: usize,
    resampler: Option<SincFixedIn<f32>>,
    /// Mono input staged for the resampler (it wants fixed-size blocks).
    staged: Vec<f32>,
    block: usize,
}

impl ToMono16k {
    /// `in_rate` / `channels` describe the capture stream.
    #[must_use]
    pub fn new(in_rate: u32, channels: u16) -> Self {
        let channels = channels.max(1) as usize;
        let resampler = (in_rate != SAMPLE_RATE).then(|| {
            let params = SincInterpolationParameters {
                sinc_len: 128,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 128,
                window: WindowFunction::BlackmanHarris2,
            };
            let block = 1024;
            SincFixedIn::<f32>::new(
                f64::from(SAMPLE_RATE) / f64::from(in_rate),
                2.0,
                params,
                block,
                1,
            )
            .expect("valid resampler params")
        });
        Self {
            channels,
            resampler,
            staged: Vec::new(),
            block: 1024,
        }
    }

    /// Push interleaved device frames; returns any 16 kHz mono samples produced.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<f32> {
        // Downmix to mono.
        let mono: Vec<f32> = if self.channels == 1 {
            interleaved.to_vec()
        } else {
            interleaved
                .chunks_exact(self.channels)
                .map(|f| f.iter().sum::<f32>() / self.channels as f32)
                .collect()
        };

        let Some(resampler) = self.resampler.as_mut() else {
            return mono;
        };

        self.staged.extend_from_slice(&mono);
        let mut out = Vec::new();
        while self.staged.len() >= self.block {
            let chunk: Vec<f32> = self.staged.drain(..self.block).collect();
            let done = resampler
                .process(&[chunk], None)
                .expect("resampler process");
            out.extend_from_slice(&done[0]);
        }
        out
    }
}

/// A generic streaming mono resampler (`in_rate` → `out_rate`). Pass-through
/// when the rates match. Feed arbitrary-length blocks with [`Resampler16::push`].
pub struct Resampler16 {
    resampler: Option<SincFixedIn<f32>>,
    staged: Vec<f32>,
    block: usize,
}

impl Resampler16 {
    /// Build for `in_rate` → `out_rate` (mono).
    #[must_use]
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let block = 1024;
        let resampler = (in_rate != out_rate).then(|| {
            let params = SincInterpolationParameters {
                sinc_len: 128,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 128,
                window: WindowFunction::BlackmanHarris2,
            };
            SincFixedIn::<f32>::new(
                f64::from(out_rate) / f64::from(in_rate),
                2.0,
                params,
                block,
                1,
            )
            .expect("valid resampler params")
        });
        Self {
            resampler,
            staged: Vec::new(),
            block,
        }
    }

    /// Feed mono samples; return whatever output samples are ready.
    pub fn push(&mut self, samples: &[f32]) -> Vec<f32> {
        let Some(resampler) = self.resampler.as_mut() else {
            return samples.to_vec();
        };
        self.staged.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.staged.len() >= self.block {
            let chunk: Vec<f32> = self.staged.drain(..self.block).collect();
            let done = resampler
                .process(&[chunk], None)
                .expect("resampler process");
            out.extend_from_slice(&done[0]);
        }
        out
    }

    /// Flush the remaining staged samples (zero-padded to one block).
    pub fn flush(&mut self) -> Vec<f32> {
        let Some(resampler) = self.resampler.as_mut() else {
            return std::mem::take(&mut self.staged);
        };
        if self.staged.is_empty() {
            return Vec::new();
        }
        let mut chunk = std::mem::take(&mut self.staged);
        let real = chunk.len();
        chunk.resize(self.block, 0.0);
        let done = resampler.process(&[chunk], None).expect("resampler flush");
        let want = real * done[0].len() / self.block;
        done[0].iter().take(want.max(1)).copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_already_16k_mono() {
        let mut c = ToMono16k::new(SAMPLE_RATE, 1);
        let out = c.push(&[0.1, 0.2, 0.3]);
        assert_eq!(out, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn downmix_stereo_to_mono() {
        let mut c = ToMono16k::new(SAMPLE_RATE, 2);
        // L=0.0 R=1.0 → 0.5
        let out = c.push(&[0.0, 1.0, 0.2, 0.4]);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.5).abs() < 1e-6);
        assert!((out[1] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn resamples_48k_to_16k_thirds_the_length() {
        let mut c = ToMono16k::new(48_000, 1);
        // 1 s of 48 kHz sine → ~16 000 samples out (allow resampler warm-up slack).
        let input: Vec<f32> = (0..48_000)
            .map(|i| (i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin())
            .collect();
        let out = c.push(&input);
        let ratio = out.len() as f32 / 16_000.0;
        assert!((0.9..=1.1).contains(&ratio), "got {} samples", out.len());
    }

    #[test]
    fn resampler16_passthrough_and_upsample() {
        let mut same = Resampler16::new(24_000, 24_000);
        assert_eq!(same.push(&[0.1, 0.2]), vec![0.1, 0.2]);

        let mut up = Resampler16::new(24_000, 48_000);
        let input: Vec<f32> = (0..24_000)
            .map(|i| (i as f32 / 24_000.0 * 220.0 * std::f32::consts::TAU).sin())
            .collect();
        let mut out = up.push(&input);
        out.extend(up.flush());
        let ratio = out.len() as f32 / 48_000.0;
        assert!((0.9..=1.1).contains(&ratio), "got {} samples", out.len());
    }
}
