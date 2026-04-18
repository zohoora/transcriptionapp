//! Structured mismatch reports for harness equivalence tests.
//!
//! First-divergence semantics: the comparator walks in canonical order and
//! stops at the first disagreement — cascading downstream differences are
//! suppressed so the report points at the one place behavior diverged.

use super::captured_event::CapturedEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum MismatchKind {
    DetectionDecision {
        at_segment_index: u64,
        expected: serde_json::Value,
        actual: serde_json::Value,
    },
    MergeDecision {
        expected: serde_json::Value,
        actual: serde_json::Value,
    },
    MultiPatientSplit {
        expected: serde_json::Value,
        actual: serde_json::Value,
    },
    ArchiveField {
        session: String,
        field: String,
        expected: serde_json::Value,
        actual: serde_json::Value,
    },
    MissingArchiveFile {
        session: String,
        file: String,
    },
    UnexpectedArchiveFile {
        session: String,
        file: String,
    },
    MissingSession {
        session: String,
    },
    EventPayload {
        event_index: usize,
        field: String,
        expected: serde_json::Value,
        actual: serde_json::Value,
    },
    MissingEvent {
        expected_event_name: String,
        at_event_index: usize,
    },
    UnexpectedEvent {
        actual_event_name: String,
        at_event_index: usize,
    },
    UnmatchedPrompt {
        task: String,
        prompt_hash: String,
    },
    OrchestratorPanic {
        message: String,
    },
    OrchestratorTimeout {
        limit_secs: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Verdict {
    Equivalent,
    Divergent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Divergence {
    pub kind: MismatchKind,
    pub preceding_events: Vec<CapturedEvent>,
    pub drill_in_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MismatchReport {
    pub test_id: String,
    pub bundle_path: String,
    pub verdict: Verdict,
    pub first_divergence: Option<Divergence>,
    pub summary_one_liner: String,
}

impl MismatchReport {
    pub fn equivalent(test_id: &str, bundle_path: &str) -> Self {
        Self {
            test_id: test_id.into(),
            bundle_path: bundle_path.into(),
            verdict: Verdict::Equivalent,
            first_divergence: None,
            summary_one_liner: format!("{}: Equivalent", test_id),
        }
    }

    pub fn divergent(
        test_id: &str,
        bundle_path: &str,
        kind: MismatchKind,
        preceding_events: Vec<CapturedEvent>,
    ) -> Self {
        let summary = summarize_one_liner(test_id, &kind);
        let drill_in_command = format!(
            "HARNESS_FOCUS=1 cargo test --test harness_per_encounter {} -- --nocapture",
            test_id
        );
        Self {
            test_id: test_id.into(),
            bundle_path: bundle_path.into(),
            verdict: Verdict::Divergent,
            first_divergence: Some(Divergence {
                kind,
                preceding_events,
                drill_in_command,
            }),
            summary_one_liner: summary,
        }
    }

    /// Panic with summary + write full JSON artifact if Divergent.
    /// Used at the end of `EncounterHarness::run().expect_equivalent()`.
    pub fn expect_equivalent(self) {
        if let Verdict::Divergent = self.verdict {
            let artifact_dir = std::path::PathBuf::from("target/harness-report");
            let _ = std::fs::create_dir_all(&artifact_dir);
            let path = artifact_dir.join(format!("{}.json", self.test_id));
            let pretty = serde_json::to_string_pretty(&self).unwrap_or_default();
            let _ = std::fs::write(&path, pretty);

            eprintln!("{}", self.summary_one_liner);
            eprintln!("Full report: {}", path.display());
            if let Some(d) = &self.first_divergence {
                eprintln!("Drill-in: {}", d.drill_in_command);
            }
            panic!("harness detected divergence: {}", self.summary_one_liner);
        }
    }
}

fn summarize_one_liner(test_id: &str, kind: &MismatchKind) -> String {
    match kind {
        MismatchKind::DetectionDecision { at_segment_index, .. } => {
            format!("{}: detection decision differs at segment {}", test_id, at_segment_index)
        }
        MismatchKind::MergeDecision { .. } => {
            format!("{}: merge decision differs", test_id)
        }
        MismatchKind::MultiPatientSplit { .. } => {
            format!("{}: multi-patient split differs", test_id)
        }
        MismatchKind::ArchiveField { session, field, .. } => {
            format!("{}: archive field '{}' differs for session {}", test_id, field, session)
        }
        MismatchKind::MissingArchiveFile { session, file } => {
            format!("{}: expected file {} missing from session {}", test_id, file, session)
        }
        MismatchKind::UnexpectedArchiveFile { session, file } => {
            format!("{}: unexpected file {} in session {}", test_id, file, session)
        }
        MismatchKind::MissingSession { session } => {
            format!("{}: session {} not found in actual archive", test_id, session)
        }
        MismatchKind::EventPayload { event_index, field, .. } => {
            format!("{}: event #{} payload '{}' differs", test_id, event_index, field)
        }
        MismatchKind::MissingEvent { expected_event_name, at_event_index } => {
            format!("{}: expected event '{}' at index {} was missing", test_id, expected_event_name, at_event_index)
        }
        MismatchKind::UnexpectedEvent { actual_event_name, at_event_index } => {
            format!("{}: unexpected event '{}' at index {}", test_id, actual_event_name, at_event_index)
        }
        MismatchKind::UnmatchedPrompt { task, prompt_hash } => {
            format!("{}: no recorded response for task='{}' prompt_hash={}", test_id, task, prompt_hash)
        }
        MismatchKind::OrchestratorPanic { message } => {
            format!("{}: orchestrator panicked: {}", test_id, message)
        }
        MismatchKind::OrchestratorTimeout { limit_secs } => {
            format!("{}: orchestrator timed out after {}s virtual", test_id, limit_secs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_report_serializes() {
        let r = MismatchReport::equivalent("test_001", "path/to/bundle.json");
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("Equivalent"));
    }

    #[test]
    fn divergent_report_has_segment_in_summary() {
        let r = MismatchReport::divergent(
            "test_001",
            "bundle.json",
            MismatchKind::DetectionDecision {
                at_segment_index: 42,
                expected: serde_json::json!({"complete": true}),
                actual: serde_json::json!({"complete": false}),
            },
            vec![],
        );
        assert!(r.summary_one_liner.contains("segment 42"), "summary: {}", r.summary_one_liner);
        assert_eq!(r.verdict, Verdict::Divergent);
    }

    #[test]
    fn drill_in_command_is_re_runnable() {
        let r = MismatchReport::divergent(
            "encounter_abc",
            "bundle.json",
            MismatchKind::ArchiveField {
                session: "abc".into(),
                field: "patient_name".into(),
                expected: serde_json::json!("Jane"),
                actual: serde_json::json!("John"),
            },
            vec![],
        );
        let cmd = &r.first_divergence.unwrap().drill_in_command;
        assert!(cmd.contains("cargo test"));
        assert!(cmd.contains("encounter_abc"));
        assert!(cmd.contains("HARNESS_FOCUS"));
    }
}
