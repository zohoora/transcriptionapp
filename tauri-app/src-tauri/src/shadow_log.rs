//! Shadow Mode Logging
//!
//! Records shadow detection decisions for dual-method comparison in continuous mode.
//! The shadow method observes but never affects the encounter lifecycle — it logs
//! what it *would* have done so operators can compare detection accuracy.
//!
//! CSV logs are written to `~/.transcriptionapp/shadow/YYYY-MM-DD.csv` with daily rotation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

// ============================================================================
// Types
// ============================================================================

/// What the shadow method decided
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowOutcome {
    #[serde(rename = "would_split")]
    WouldSplit,
    #[serde(rename = "would_not_split")]
    WouldNotSplit,
}

impl ShadowOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShadowOutcome::WouldSplit => "would_split",
            ShadowOutcome::WouldNotSplit => "would_not_split",
        }
    }
}

/// A single shadow detection decision (full detail, used in-memory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowDecision {
    pub timestamp: DateTime<Utc>,
    /// Which method is the shadow ("llm" or "sensor")
    pub shadow_method: String,
    /// Which method is the active one
    pub active_method: String,
    pub outcome: ShadowOutcome,
    /// LLM confidence (0.0-1.0), or 1.0 for sensor
    pub confidence: Option<f64>,
    pub buffer_word_count: usize,
    /// Last segment index in the buffer at decision time
    pub buffer_last_segment: Option<u64>,
}

/// Summary of a shadow decision (stored in archive metadata per encounter)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowDecisionSummary {
    pub timestamp: String,
    pub outcome: String,
    pub confidence: Option<f64>,
    pub buffer_word_count: usize,
}

impl From<&ShadowDecision> for ShadowDecisionSummary {
    fn from(d: &ShadowDecision) -> Self {
        Self {
            timestamp: d.timestamp.to_rfc3339(),
            outcome: d.outcome.as_str().to_string(),
            confidence: d.confidence,
            buffer_word_count: d.buffer_word_count,
        }
    }
}

/// Per-encounter comparison data (stored in archive metadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowEncounterComparison {
    /// Which method was the shadow for this encounter
    pub shadow_method: String,
    /// Shadow decisions recorded during this encounter
    pub decisions: Vec<ShadowDecisionSummary>,
    /// When the active method triggered the split (ISO timestamp)
    pub active_split_at: String,
    /// Did the shadow method also want to split near the active split time?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_agreed: Option<bool>,
}

// ============================================================================
// CSV Logger
// ============================================================================

/// CSV logger for shadow decisions — daily rotation to ~/.transcriptionapp/shadow/
pub struct ShadowCsvLogger {
    log_dir: PathBuf,
    current_date: String,
    file: Option<std::fs::File>,
}

impl ShadowCsvLogger {
    /// Create a new logger. Creates the log directory if needed.
    pub fn new() -> Result<Self, String> {
        let log_dir = dirs::home_dir()
            .ok_or("No home directory")?
            .join(".transcriptionapp")
            .join("shadow");

        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create shadow log dir: {}", e))?;

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut logger = ShadowCsvLogger {
            log_dir,
            current_date: today,
            file: None,
        };
        logger.open_file()?;
        Ok(logger)
    }

    fn open_file(&mut self) -> Result<(), String> {
        let path = self.log_dir.join(format!("{}.csv", self.current_date));
        let write_header = !path.exists();

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("Failed to open shadow CSV log: {}", e))?;

        if write_header {
            use std::io::Write;
            let mut f = &file;
            let _ = writeln!(
                f,
                "timestamp_utc,shadow_method,active_method,outcome,confidence,buffer_words,buffer_last_segment"
            );
        }

        self.file = Some(file);
        info!("Shadow CSV log opened: {}.csv", self.current_date);
        Ok(())
    }

    /// Write a shadow decision to CSV
    pub fn write_decision(&mut self, decision: &ShadowDecision) {
        let now_utc = decision.timestamp;

        // Check for midnight rotation
        let today = now_utc.format("%Y-%m-%d").to_string();
        if today != self.current_date {
            self.current_date = today;
            if let Err(e) = self.open_file() {
                warn!("Failed to rotate shadow CSV log: {}", e);
                return;
            }
            info!("Shadow CSV log rotated to {}.csv", self.current_date);
        }

        if let Some(ref mut file) = self.file {
            use std::io::Write;
            let ts = now_utc.format("%Y-%m-%dT%H:%M:%S%.3fZ");
            let confidence_str = decision
                .confidence
                .map(|c| format!("{:.3}", c))
                .unwrap_or_default();
            let segment_str = decision
                .buffer_last_segment
                .map(|s| s.to_string())
                .unwrap_or_default();

            let _ = writeln!(
                file,
                "{},{},{},{},{},{},{}",
                ts,
                decision.shadow_method,
                decision.active_method,
                decision.outcome.as_str(),
                confidence_str,
                decision.buffer_word_count,
                segment_str,
            );
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_outcome_serialization() {
        let split = ShadowOutcome::WouldSplit;
        let json = serde_json::to_string(&split).unwrap();
        assert_eq!(json, "\"would_split\"");

        let not_split = ShadowOutcome::WouldNotSplit;
        let json = serde_json::to_string(&not_split).unwrap();
        assert_eq!(json, "\"would_not_split\"");
    }

    #[test]
    fn test_shadow_outcome_deserialization() {
        let split: ShadowOutcome = serde_json::from_str("\"would_split\"").unwrap();
        assert_eq!(split, ShadowOutcome::WouldSplit);

        let not_split: ShadowOutcome = serde_json::from_str("\"would_not_split\"").unwrap();
        assert_eq!(not_split, ShadowOutcome::WouldNotSplit);
    }

    #[test]
    fn test_shadow_decision_roundtrip() {
        let decision = ShadowDecision {
            timestamp: Utc::now(),
            shadow_method: "llm".to_string(),
            active_method: "sensor".to_string(),
            outcome: ShadowOutcome::WouldSplit,
            confidence: Some(0.85),
            buffer_word_count: 500,
            buffer_last_segment: Some(42),
        };

        let json = serde_json::to_string(&decision).unwrap();
        let deserialized: ShadowDecision = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.shadow_method, "llm");
        assert_eq!(deserialized.active_method, "sensor");
        assert_eq!(deserialized.outcome, ShadowOutcome::WouldSplit);
        assert_eq!(deserialized.confidence, Some(0.85));
        assert_eq!(deserialized.buffer_word_count, 500);
        assert_eq!(deserialized.buffer_last_segment, Some(42));
    }

    #[test]
    fn test_shadow_decision_summary_from() {
        let decision = ShadowDecision {
            timestamp: Utc::now(),
            shadow_method: "sensor".to_string(),
            active_method: "llm".to_string(),
            outcome: ShadowOutcome::WouldNotSplit,
            confidence: None,
            buffer_word_count: 200,
            buffer_last_segment: None,
        };

        let summary = ShadowDecisionSummary::from(&decision);
        assert_eq!(summary.outcome, "would_not_split");
        assert_eq!(summary.confidence, None);
        assert_eq!(summary.buffer_word_count, 200);
    }

    #[test]
    fn test_shadow_encounter_comparison() {
        let comparison = ShadowEncounterComparison {
            shadow_method: "llm".to_string(),
            decisions: vec![
                ShadowDecisionSummary {
                    timestamp: "2026-02-20T10:00:00Z".to_string(),
                    outcome: "would_not_split".to_string(),
                    confidence: Some(0.3),
                    buffer_word_count: 100,
                },
                ShadowDecisionSummary {
                    timestamp: "2026-02-20T10:02:00Z".to_string(),
                    outcome: "would_split".to_string(),
                    confidence: Some(0.9),
                    buffer_word_count: 350,
                },
            ],
            active_split_at: "2026-02-20T10:01:30Z".to_string(),
            shadow_agreed: Some(true),
        };

        let json = serde_json::to_string(&comparison).unwrap();
        let deserialized: ShadowEncounterComparison = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.shadow_method, "llm");
        assert_eq!(deserialized.decisions.len(), 2);
        assert_eq!(deserialized.shadow_agreed, Some(true));
    }

    #[test]
    fn test_shadow_encounter_comparison_without_agreement() {
        let comparison = ShadowEncounterComparison {
            shadow_method: "sensor".to_string(),
            decisions: vec![],
            active_split_at: "2026-02-20T10:00:00Z".to_string(),
            shadow_agreed: None,
        };

        let json = serde_json::to_string(&comparison).unwrap();
        // shadow_agreed should be omitted when None
        assert!(!json.contains("shadow_agreed"));
    }

    #[test]
    fn test_csv_line_format() {
        // Verify format manually
        let ts = "2026-02-20T10:30:00.000Z";
        let line = format!(
            "{},{},{},{},{},{},{}",
            ts, "llm", "sensor", "would_split", "0.850", 500, 42
        );

        assert!(line.contains("2026-02-20T10:30:00.000Z"));
        assert!(line.contains("llm,sensor"));
        assert!(line.contains("would_split"));
        assert!(line.contains("0.850"));
        assert!(line.contains(",500,42"));
    }
}
