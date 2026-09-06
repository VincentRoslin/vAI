//! Pre-transfer disk-budget guard (ADR-0008). A download is refused **before any
//! byte is fetched** if it would either overrun the configured model-dir budget
//! or leave less than `models.min_free_gb` free on the volume.

use std::path::Path;

use walkdir::WalkDir;

use crate::ipc::{AppError, AppResult};

const GB: u64 = 1024 * 1024 * 1024;

/// Total bytes used by files under `dir` (0 if it does not exist yet).
#[must_use]
pub fn dir_usage(dir: &Path) -> u64 {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|e| e.metadata().ok())
        .filter(std::fs::Metadata::is_file)
        .map(|m| m.len())
        .sum()
}

/// Free bytes on the volume containing `dir` (or its nearest existing ancestor).
fn free_space(dir: &Path) -> AppResult<u64> {
    let mut probe = dir;
    loop {
        if probe.exists() {
            return fs4::available_space(probe)
                .map_err(|e| AppError::internal("query free disk space", e));
        }
        match probe.parent() {
            Some(parent) => probe = parent,
            None => {
                return Err(AppError::internal(
                    "query free disk space",
                    "no existing ancestor",
                ))
            }
        }
    }
}

/// Refuse a `file_size`-byte download that would break either budget.
///
/// # Errors
/// [`AppError::ResourceExhausted`] naming the shortfall.
pub fn check_budget(
    file_size: u64,
    models_dir: &Path,
    budget_gb: u32,
    min_free_gb: u32,
) -> AppResult<()> {
    let budget = u64::from(budget_gb) * GB;
    let used = dir_usage(models_dir);
    if used + file_size > budget {
        return Err(AppError::ResourceExhausted(format!(
            "download ({:.1} GB) would exceed the model-dir budget ({budget_gb} GB; {:.1} GB used)",
            bytes_gb(file_size),
            bytes_gb(used),
        )));
    }

    let free = free_space(models_dir)?;
    let min_free = u64::from(min_free_gb) * GB;
    if free < file_size + min_free {
        return Err(AppError::ResourceExhausted(format!(
            "download ({:.1} GB) would leave less than the {min_free_gb} GB free-space margin ({:.1} GB free now)",
            bytes_gb(file_size),
            bytes_gb(free),
        )));
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn bytes_gb(bytes: u64) -> f64 {
    bytes as f64 / GB as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_budget_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        // 2 GB file, 1 GB budget.
        let err = check_budget(2 * GB, dir.path(), 1, 1).unwrap_err();
        assert!(matches!(&err, AppError::ResourceExhausted(m) if m.contains("budget")));
    }

    #[test]
    fn insufficient_free_space_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        // Huge min-free margin — nothing fits.
        let err = check_budget(GB, dir.path(), 100_000, 1_000_000).unwrap_err();
        assert!(matches!(&err, AppError::ResourceExhausted(m) if m.contains("free-space")));
    }

    #[test]
    fn a_fitting_download_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        // 1 MB file, 100 GB budget, 1 GB margin — fine on any dev machine.
        check_budget(1024 * 1024, dir.path(), 100, 1).expect("fits");
    }

    #[test]
    fn dir_usage_sums_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), vec![0u8; 1000]).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/b"), vec![0u8; 2345]).unwrap();
        assert_eq!(dir_usage(dir.path()), 3345);
        assert_eq!(dir_usage(&dir.path().join("missing")), 0);
    }
}
