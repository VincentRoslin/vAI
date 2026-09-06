//! Endpointed-segment assembly: pre-roll + speech samples → a 16 kHz mono
//! `s16le` WAV under the app cache dir, handed to the STT worker by path
//! (Article I — Rust dictates the path; the worker only reads it).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::path::{Path, PathBuf};

use crate::ipc::{AppError, AppResult};
use crate::voice::SAMPLE_RATE;

/// Write `samples` (mono f32 in `-1.0..=1.0`, already at [`SAMPLE_RATE`]) as a
/// 16-bit PCM WAV at `dir/<name>.wav`.
///
/// # Errors
/// [`AppError::internal`] on any I/O failure.
pub fn write_wav(dir: &Path, name: &str, samples: &[f32]) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| AppError::internal("create voice temp dir", e))?;
    let path = dir.join(format!("{name}.wav"));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(&path, spec).map_err(|e| AppError::internal("create wav", e))?;
    for &s in samples {
        let clamped = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        writer
            .write_sample(clamped)
            .map_err(|e| AppError::internal("write wav sample", e))?;
    }
    writer
        .finalize()
        .map_err(|e| AppError::internal("finalize wav", e))?;
    Ok(path)
}

/// Best-effort delete of a temp segment file.
pub fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_duration_and_format() {
        let dir = tempfile::tempdir().unwrap();
        let secs = 1.5_f32;
        let n = (secs * SAMPLE_RATE as f32) as usize;
        let samples: Vec<f32> = (0..n).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let path = write_wav(dir.path(), "seg", &samples).unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().sample_rate, SAMPLE_RATE);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.len() as usize, n);

        cleanup(&path);
        assert!(!path.exists());
    }
}
