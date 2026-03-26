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
use tracing::warn;

const LOG_FILENAME: &str = "day_log.jsonl";

/// Appends structured JSONL events to a day-level archive directory.
/// Unlike `PipelineLogger` (per-session), this writes to a fixed path
/// derived from today's date: `~/.transcriptionapp/archive/YYYY/MM/DD/day_log.jsonl`.
pub struct DayLogger {
    /// Today's log file path: ~/.transcriptionapp/archive/YYYY/MM/DD/day_log.jsonl
    path: PathBuf,
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
        let now = Local::now();
        let date_path = home
            .join(".transcriptionapp")
            .join("archive")
            .join(now.format("%Y").to_string())
            .join(now.format("%m").to_string())
            .join(now.format("%d").to_string());

        if let Err(e) = std::fs::create_dir_all(&date_path) {
            warn!("Day logger: failed to create directory {}: {}", date_path.display(), e);
            return None;
        }

        Some(Self {
            path: date_path.join(LOG_FILENAME),
        })
    }

    /// Create a day logger with an explicit path (for testing).
    #[cfg(test)]
    fn new_with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Append a day event to the log file.
    /// Never panics or blocks on I/O errors — logs a warning instead.
    pub fn log(&self, event: DayEvent) {
        let line = match serde_json::to_string(&event) {
            Ok(l) => l,
            Err(e) => {
                warn!("Day log serialization failed: {}", e);
                return;
            }
        };

        if let Err(e) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
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
}
