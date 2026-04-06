//! Day-level orchestration logging for continuous mode.
//!
//! Appends one JSONL line per major pipeline event to a day-level log file:
//! `~/.transcriptionapp/archive/YYYY/MM/DD/day_log.jsonl`
//!
//! Events include continuous mode start/stop, encounter splits, merges,
//! clinical checks, and SOAP generation results.

use chrono::Local;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{info, warn};

const LOG_FILENAME: &str = "day_log.jsonl";

/// Mutable inner state, protected by a Mutex to allow `log(&self)` from an `Arc<DayLogger>`.
struct DayLoggerInner {
    /// The date string for which `path` was computed (e.g. `"2026-03-31"`).
    current_date: String,
    /// Current log file path, recomputed on midnight rotation.
    path: PathBuf,
}

/// Appends structured JSONL events to a day-level archive directory.
/// Unlike `PipelineLogger` (per-session), this writes to a fixed path
/// derived from today's date: `~/.transcriptionapp/archive/YYYY/MM/DD/day_log.jsonl`.
///
/// The file path rotates automatically at midnight: the first `log()` call after
/// midnight recomputes the path and begins writing to the new day's file.
pub struct DayLogger {
    /// Base archive directory: `~/.transcriptionapp/archive/`
    archive_root: PathBuf,
    inner: Mutex<DayLoggerInner>,
}

/// A typed day-level event.
#[derive(Debug, Serialize)]
#[serde(tag = "event")]
pub enum DayEvent {
    #[serde(rename = "continuous_mode_started")]
    ContinuousModeStarted {
        ts: String,
        config: serde_json::Value,
    },
    #[serde(rename = "encounter_split")]
    EncounterSplit {
        ts: String,
        session_id: String,
        encounter_number: u32,
        trigger: String,
        word_count: usize,
        detection_method: String,
    },
    #[serde(rename = "encounter_merged")]
    EncounterMerged {
        ts: String,
        new_session_id: String,
        prev_session_id: String,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        gate_type: Option<String>,
    },
    #[serde(rename = "clinical_check_result")]
    ClinicalCheckResult {
        ts: String,
        session_id: String,
        is_clinical: bool,
    },
    #[serde(rename = "soap_generated")]
    SoapGenerated {
        ts: String,
        session_id: String,
        latency_ms: u64,
        success: bool,
    },
    #[serde(rename = "retrospective_split")]
    RetrospectiveSplit {
        ts: String,
        session_id: String,
        split_into: Vec<String>,
    },
    #[serde(rename = "idle_buffer_cleared")]
    IdleBufferCleared {
        ts: String,
        word_count: usize,
        buffer_age_secs: i64,
    },
    #[serde(rename = "billing_extracted")]
    BillingExtracted {
        ts: String,
        session_id: String,
        codes_count: u32,
        total_amount_cents: u32,
        latency_ms: u64,
        success: bool,
    },
    #[serde(rename = "billing_invalidated")]
    BillingInvalidated {
        ts: String,
        session_id: String,
        reason: String,
    },
    #[serde(rename = "continuous_mode_stopped")]
    ContinuousModeStopped {
        ts: String,
        total_encounters: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        flush_session_id: Option<String>,
    },
}

impl DayLogger {
    /// Create a day logger for today's date.
    /// Creates the archive date directory if it doesn't exist.
    /// Returns `None` if the home directory cannot be determined or the
    /// directory cannot be created.
    pub fn new() -> Option<Self> {
        let home = dirs::home_dir()?;
        let archive_root = home.join(".transcriptionapp").join("archive");
        let now = Local::now();
        let current_date = now.format("%Y-%m-%d").to_string();
        let date_path = archive_root
            .join(now.format("%Y").to_string())
            .join(now.format("%m").to_string())
            .join(now.format("%d").to_string());

        if let Err(e) = std::fs::create_dir_all(&date_path) {
            warn!("Day logger: failed to create directory {}: {}", date_path.display(), e);
            return None;
        }

        Some(Self {
            archive_root,
            inner: Mutex::new(DayLoggerInner {
                current_date,
                path: date_path.join(LOG_FILENAME),
            }),
        })
    }

    /// Create a day logger with an explicit path (for testing).
    /// Uses the path's parent as the archive root so rotation tests can work.
    #[cfg(test)]
    fn new_with_path(path: PathBuf) -> Self {
        let archive_root = path.parent().unwrap_or(&path).to_path_buf();
        // Use a sentinel date so tests can force rotation by passing a different date string.
        let current_date = Local::now().format("%Y-%m-%d").to_string();
        Self {
            archive_root,
            inner: Mutex::new(DayLoggerInner {
                current_date,
                path,
            }),
        }
    }

    /// Create a day logger rooted at `archive_root` with `current_date` pre-set to a
    /// past date, so the next `log()` call will immediately trigger rotation.
    /// Used to test midnight rotation without time manipulation.
    #[cfg(test)]
    fn new_for_rotation_test(archive_root: PathBuf, stale_date: &str) -> Self {
        // Build a plausible (but stale) path so the inner state is consistent.
        let parts: Vec<&str> = stale_date.splitn(3, '-').collect();
        let stale_path = if parts.len() == 3 {
            archive_root
                .join(parts[0])
                .join(parts[1])
                .join(parts[2])
                .join(LOG_FILENAME)
        } else {
            archive_root.join(LOG_FILENAME)
        };
        Self {
            archive_root,
            inner: Mutex::new(DayLoggerInner {
                current_date: stale_date.to_string(),
                path: stale_path,
            }),
        }
    }

    /// Recompute the log path for `date_str` (format: `"YYYY-MM-DD"`) and
    /// ensure its directory exists.  Updates `inner` on success.
    fn rotate_to_date(archive_root: &PathBuf, inner: &mut DayLoggerInner, date_str: &str) {
        // Parse YYYY-MM-DD components.
        let parts: Vec<&str> = date_str.splitn(3, '-').collect();
        if parts.len() != 3 {
            warn!("Day log: unexpected date format '{}', skipping rotation", date_str);
            return;
        }
        let (year, month, day) = (parts[0], parts[1], parts[2]);
        let date_path = archive_root.join(year).join(month).join(day);

        if let Err(e) = std::fs::create_dir_all(&date_path) {
            warn!("Day log: failed to create directory {} for rotation: {}", date_path.display(), e);
            return;
        }

        inner.current_date = date_str.to_string();
        inner.path = date_path.join(LOG_FILENAME);
        info!("Day log rotated to {}", inner.path.display());
    }

    /// Append a day event to the log file.
    /// Automatically rotates to a new file if the calendar date has changed since
    /// the last write (midnight rotation).
    /// Never panics or blocks on I/O errors — logs a warning instead.
    pub fn log(&self, event: DayEvent) {
        let line = match serde_json::to_string(&event) {
            Ok(l) => l,
            Err(e) => {
                warn!("Day log serialization failed: {}", e);
                return;
            }
        };

        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("Day log mutex poisoned: {}", e);
                return;
            }
        };

        // Check for midnight rotation.
        let today = Local::now().format("%Y-%m-%d").to_string();
        if today != inner.current_date {
            Self::rotate_to_date(&self.archive_root, &mut inner, &today);
        }

        if let Err(e) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&inner.path)
            .and_then(|mut f| writeln!(f, "{}", line))
        {
            warn!("Day log write failed: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::fs;

    #[test]
    fn test_day_event_serialization_continuous_mode_started() {
        let event = DayEvent::ContinuousModeStarted {
            ts: "2026-03-10T08:00:00Z".to_string(),
            config: serde_json::json!({"encounter_check_interval_secs": 120}),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "continuous_mode_started");
        assert_eq!(json["ts"], "2026-03-10T08:00:00Z");
        assert_eq!(json["config"]["encounter_check_interval_secs"], 120);
    }

    #[test]
    fn test_day_event_serialization_encounter_split() {
        let event = DayEvent::EncounterSplit {
            ts: "2026-03-10T09:15:00Z".to_string(),
            session_id: "abc-123".to_string(),
            encounter_number: 3,
            trigger: "hybrid_llm".to_string(),
            word_count: 1500,
            detection_method: "hybrid".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "encounter_split");
        assert_eq!(json["session_id"], "abc-123");
        assert_eq!(json["encounter_number"], 3);
        assert_eq!(json["trigger"], "hybrid_llm");
        assert_eq!(json["word_count"], 1500);
        assert_eq!(json["detection_method"], "hybrid");
    }

    #[test]
    fn test_day_event_serialization_encounter_merged() {
        let event = DayEvent::EncounterMerged {
            ts: "2026-03-10T09:20:00Z".to_string(),
            new_session_id: "def-456".to_string(),
            prev_session_id: "abc-123".to_string(),
            reason: "same_encounter".to_string(),
            gate_type: Some("confidence".to_string()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "encounter_merged");
        assert_eq!(json["new_session_id"], "def-456");
        assert_eq!(json["prev_session_id"], "abc-123");
        assert_eq!(json["gate_type"], "confidence");
    }

    #[test]
    fn test_day_event_serialization_encounter_merged_no_gate() {
        let event = DayEvent::EncounterMerged {
            ts: "2026-03-10T09:20:00Z".to_string(),
            new_session_id: "def-456".to_string(),
            prev_session_id: "abc-123".to_string(),
            reason: "same_encounter".to_string(),
            gate_type: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        // gate_type should be absent (skip_serializing_if = "Option::is_none")
        assert!(!json.contains("gate_type"));
    }

    #[test]
    fn test_day_event_serialization_clinical_check_result() {
        let event = DayEvent::ClinicalCheckResult {
            ts: "2026-03-10T09:25:00Z".to_string(),
            session_id: "abc-123".to_string(),
            is_clinical: true,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "clinical_check_result");
        assert_eq!(json["is_clinical"], true);
    }

    #[test]
    fn test_day_event_serialization_soap_generated() {
        let event = DayEvent::SoapGenerated {
            ts: "2026-03-10T09:30:00Z".to_string(),
            session_id: "abc-123".to_string(),
            latency_ms: 8500,
            success: true,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "soap_generated");
        assert_eq!(json["latency_ms"], 8500);
        assert_eq!(json["success"], true);
    }

    #[test]
    fn test_day_event_serialization_retrospective_split() {
        let event = DayEvent::RetrospectiveSplit {
            ts: "2026-03-10T10:00:00Z".to_string(),
            session_id: "abc-123".to_string(),
            split_into: vec!["abc-123-a".to_string(), "abc-123-b".to_string()],
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "retrospective_split");
        assert_eq!(json["split_into"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_day_event_serialization_continuous_mode_stopped() {
        let event = DayEvent::ContinuousModeStopped {
            ts: "2026-03-10T17:00:00Z".to_string(),
            total_encounters: 12,
            flush_session_id: Some("final-session".to_string()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "continuous_mode_stopped");
        assert_eq!(json["total_encounters"], 12);
        assert_eq!(json["flush_session_id"], "final-session");
    }

    #[test]
    fn test_day_event_serialization_continuous_mode_stopped_no_flush() {
        let event = DayEvent::ContinuousModeStopped {
            ts: "2026-03-10T17:00:00Z".to_string(),
            total_encounters: 0,
            flush_session_id: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        // flush_session_id should be absent
        assert!(!json.contains("flush_session_id"));
    }

    #[test]
    fn test_day_logger_writes_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join(LOG_FILENAME);
        let logger = DayLogger::new_with_path(log_path.clone());

        logger.log(DayEvent::ContinuousModeStarted {
            ts: Utc::now().to_rfc3339(),
            config: serde_json::json!({"test": true}),
        });

        assert!(log_path.exists());
        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 1);

        let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry["event"], "continuous_mode_started");
        assert_eq!(entry["config"]["test"], true);
    }

    #[test]
    fn test_multiple_events_append() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join(LOG_FILENAME);
        let logger = DayLogger::new_with_path(log_path.clone());

        logger.log(DayEvent::ContinuousModeStarted {
            ts: "2026-03-10T08:00:00Z".to_string(),
            config: serde_json::json!({}),
        });

        logger.log(DayEvent::EncounterSplit {
            ts: "2026-03-10T09:15:00Z".to_string(),
            session_id: "s1".to_string(),
            encounter_number: 1,
            trigger: "llm".to_string(),
            word_count: 800,
            detection_method: "llm".to_string(),
        });

        logger.log(DayEvent::SoapGenerated {
            ts: "2026-03-10T09:16:00Z".to_string(),
            session_id: "s1".to_string(),
            latency_ms: 5000,
            success: true,
        });

        logger.log(DayEvent::ContinuousModeStopped {
            ts: "2026-03-10T17:00:00Z".to_string(),
            total_encounters: 1,
            flush_session_id: None,
        });

        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 4);

        // Verify each line is valid JSON with correct event type
        let e0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let e1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        let e2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        let e3: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
        assert_eq!(e0["event"], "continuous_mode_started");
        assert_eq!(e1["event"], "encounter_split");
        assert_eq!(e2["event"], "soap_generated");
        assert_eq!(e3["event"], "continuous_mode_stopped");
    }

    #[test]
    fn test_log_does_not_panic_on_bad_path() {
        let logger = DayLogger::new_with_path(PathBuf::from("/nonexistent/deeply/nested/path/day_log.jsonl"));

        // This should not panic — just warn internally
        logger.log(DayEvent::ContinuousModeStarted {
            ts: "2026-03-10T08:00:00Z".to_string(),
            config: serde_json::json!({}),
        });
    }

    #[test]
    fn test_midnight_rotation_switches_file() {
        let dir = tempfile::tempdir().unwrap();
        let archive_root = dir.path().to_path_buf();

        // Initialise the logger with a stale date (yesterday) so the very first
        // log() call sees a date mismatch and rotates.
        let stale_date = "2026-03-30";
        let logger = DayLogger::new_for_rotation_test(archive_root.clone(), stale_date);

        // Write one event — this should trigger rotation to today's date.
        logger.log(DayEvent::ContinuousModeStarted {
            ts: "2026-03-31T00:00:01Z".to_string(),
            config: serde_json::json!({"rotated": true}),
        });

        // The stale date directory should NOT have a log file (rotation happened before write).
        let stale_parts: Vec<&str> = stale_date.splitn(3, '-').collect();
        let stale_log = archive_root
            .join(stale_parts[0])
            .join(stale_parts[1])
            .join(stale_parts[2])
            .join(LOG_FILENAME);
        assert!(!stale_log.exists(), "stale log file should not exist after rotation");

        // Today's directory must contain the log file with our event.
        let today = Local::now().format("%Y-%m-%d").to_string();
        let today_parts: Vec<&str> = today.splitn(3, '-').collect();
        let today_log = archive_root
            .join(today_parts[0])
            .join(today_parts[1])
            .join(today_parts[2])
            .join(LOG_FILENAME);
        assert!(today_log.exists(), "today's log file should exist after rotation");

        let content = fs::read_to_string(&today_log).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 1);
        let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry["event"], "continuous_mode_started");
        assert_eq!(entry["config"]["rotated"], true);

        // Verify internal state was updated.
        let inner = logger.inner.lock().unwrap();
        assert_eq!(inner.current_date, today);
    }
}
