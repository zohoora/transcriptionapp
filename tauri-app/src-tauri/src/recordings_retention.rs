//! Time-based pruning for the `recordings/` directory.
//!
//! Continuous mode writes `continuous_YYYYMMDD_HHMMSS.wav` for every run, and
//! session mode writes `session_YYYYMMDD_HHMMSS.wav` for every recording.
//! Neither path cleans up after itself. The session-mode files are duplicates
//! of `archive/.../audio.wav`; the continuous-mode files are the only audio
//! record of the day but are only consumed by `scripts/replay_day.py` for
//! end-to-end model comparison — their replay value drops off after the
//! forensic-review window closes (~2-4 weeks).
//!
//! See `Settings::continuous_recording_retention_days` for the policy and
//! `CODE_REVIEW_FINDINGS.md` Finding #12 for the history.

use std::path::Path;
use std::time::{Duration, SystemTime};
use tracing::warn;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct PruneSummary {
    pub files_deleted: u64,
    pub bytes_freed: u64,
    pub errors: u64,
}

pub fn prune_old_recordings(
    recordings_dir: &Path,
    retention_days: u32,
    now: SystemTime,
) -> PruneSummary {
    let mut summary = PruneSummary::default();

    if retention_days == 0 {
        return summary;
    }

    let cutoff = match now.checked_sub(Duration::from_secs(retention_days as u64 * 86_400)) {
        Some(t) => t,
        None => return summary,
    };

    let entries = match std::fs::read_dir(recordings_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(
                event = "recordings_retention_readdir_failed",
                dir = %recordings_dir.display(),
                error = %e,
            );
            return summary;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !is_pruneable(name) {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if modified >= cutoff {
            continue;
        }
        let size = metadata.len();
        match std::fs::remove_file(&path) {
            Ok(()) => {
                summary.files_deleted += 1;
                summary.bytes_freed += size;
            }
            Err(e) => {
                summary.errors += 1;
                warn!(
                    event = "recordings_retention_delete_failed",
                    file = %path.display(),
                    error = %e,
                );
            }
        }
    }

    summary
}

fn is_pruneable(name: &str) -> bool {
    name.ends_with(".wav") && (name.starts_with("continuous_") || name.starts_with("session_"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn touch(dir: &Path, name: &str, age_days: u64) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = File::create(&path).unwrap();
        f.write_all(b"\x00").unwrap();
        let mtime = SystemTime::now() - Duration::from_secs(age_days * 86_400);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(mtime)).unwrap();
        path
    }

    #[test]
    fn prunes_files_older_than_retention_window() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "continuous_20260101_120000.wav", 60);
        touch(tmp.path(), "continuous_20260415_120000.wav", 5);
        touch(tmp.path(), "session_20260102_120000.wav", 45);

        let summary = prune_old_recordings(tmp.path(), 30, SystemTime::now());

        assert_eq!(summary.files_deleted, 2);
        assert!(summary.bytes_freed >= 2);
        assert!(!tmp.path().join("continuous_20260101_120000.wav").exists());
        assert!(!tmp.path().join("session_20260102_120000.wav").exists());
        assert!(tmp.path().join("continuous_20260415_120000.wav").exists());
    }

    #[test]
    fn zero_retention_days_disables_pruning() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "continuous_20200101_120000.wav", 9999);

        let summary = prune_old_recordings(tmp.path(), 0, SystemTime::now());

        assert_eq!(summary.files_deleted, 0);
        assert!(tmp.path().join("continuous_20200101_120000.wav").exists());
    }

    #[test]
    fn ignores_non_recording_files() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "config.json", 60);
        touch(tmp.path(), "stray.wav", 60);
        touch(tmp.path(), "continuous_20260101_120000.txt", 60);
        touch(tmp.path(), "continuous_20260101_120000.wav", 60);

        let summary = prune_old_recordings(tmp.path(), 30, SystemTime::now());

        assert_eq!(summary.files_deleted, 1);
        assert!(tmp.path().join("config.json").exists());
        assert!(tmp.path().join("stray.wav").exists());
        assert!(tmp.path().join("continuous_20260101_120000.txt").exists());
    }

    #[test]
    fn missing_dir_returns_empty_summary() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let summary = prune_old_recordings(&missing, 30, SystemTime::now());
        assert_eq!(summary, PruneSummary::default());
    }

    #[test]
    fn does_not_prune_at_exact_boundary() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("continuous_20260301_120000.wav");
        File::create(&path).unwrap().write_all(b"\x00").unwrap();
        let now = SystemTime::now();
        let cutoff = now - Duration::from_secs(30 * 86_400);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(cutoff)).unwrap();

        let summary = prune_old_recordings(tmp.path(), 30, now);

        assert_eq!(summary.files_deleted, 0);
        assert!(path.exists());
    }

    #[test]
    fn is_pruneable_matches_recording_names() {
        assert!(is_pruneable("continuous_20260101_120000.wav"));
        assert!(is_pruneable("session_20260101_120000.wav"));
        assert!(!is_pruneable("audio.wav"));
        assert!(!is_pruneable("continuous_20260101_120000.txt"));
        assert!(!is_pruneable("CONTINUOUS_20260101_120000.wav"));
        assert!(!is_pruneable("Session_20260101_120000.wav"));
    }
}
