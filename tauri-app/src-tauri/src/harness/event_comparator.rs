//! Event-sequence comparator (opt-in via EquivalencePolicy::EventSequence).
//!
//! Snapshot format: the ordered sequence of `(event_name, event_type)` pairs
//! emitted by the orchestrator. We skip timestamps and most payload content —
//! those are covered by the archive comparator or are inherently
//! nondeterministic.
//!
//! Why type-only rather than full payload: for the decomposition refactor
//! we're catching control-flow regressions (missing/reordered/extra events),
//! not payload drift. A missing EncounterDetected would tell us the detector
//! branch broke; whether its session_id matches byte-for-byte is covered
//! by the archive comparator.

use super::captured_event::CapturedEvent;
use super::mismatch_report::MismatchKind;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventFingerprint {
    pub event_name: String,
    pub event_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSnapshot {
    pub events: Vec<EventFingerprint>,
}

pub fn snapshot_events(captured: &[CapturedEvent]) -> EventSnapshot {
    let events = captured
        .iter()
        .map(|e| EventFingerprint {
            event_name: e.event_name.clone(),
            event_type: e.event_type().map(String::from),
        })
        .collect();
    EventSnapshot { events }
}

/// Compare the captured event sequence against a baseline sidecar file.
///
/// Snapshot semantics: first run (or HARNESS_RECORD_BASELINES=1) records the
/// current sequence; subsequent runs verify stability.
pub fn compare_events_snapshot(
    captured: &[CapturedEvent],
    baseline_path: &Path,
) -> Result<Vec<MismatchKind>, String> {
    let actual = snapshot_events(captured);

    let record_mode =
        std::env::var("HARNESS_RECORD_BASELINES").unwrap_or_default() == "1";

    if record_mode || !baseline_path.exists() {
        if let Some(parent) = baseline_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let body = serde_json::to_string_pretty(&actual).map_err(|e| e.to_string())?;
        std::fs::write(baseline_path, body).map_err(|e| e.to_string())?;
        return Ok(vec![]);
    }

    let baseline: EventSnapshot = serde_json::from_str(
        &std::fs::read_to_string(baseline_path).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("events baseline parse: {}", e))?;

    // First-divergence walk.
    let max_len = baseline.events.len().max(actual.events.len());
    for i in 0..max_len {
        let exp = baseline.events.get(i);
        let act = actual.events.get(i);
        match (exp, act) {
            (Some(e), Some(a)) if e != a => {
                // Distinguish event_name mismatch vs event_type mismatch.
                if e.event_name != a.event_name {
                    return Ok(vec![MismatchKind::EventPayload {
                        event_index: i,
                        field: "event_name".into(),
                        expected: serde_json::json!(e.event_name),
                        actual: serde_json::json!(a.event_name),
                    }]);
                }
                return Ok(vec![MismatchKind::EventPayload {
                    event_index: i,
                    field: "type".into(),
                    expected: serde_json::json!(e.event_type),
                    actual: serde_json::json!(a.event_type),
                }]);
            }
            (Some(e), None) => {
                return Ok(vec![MismatchKind::MissingEvent {
                    expected_event_name: format!(
                        "{}[{}]",
                        e.event_name,
                        e.event_type.as_deref().unwrap_or("(none)")
                    ),
                    at_event_index: i,
                }]);
            }
            (None, Some(a)) => {
                return Ok(vec![MismatchKind::UnexpectedEvent {
                    actual_event_name: format!(
                        "{}[{}]",
                        a.event_name,
                        a.event_type.as_deref().unwrap_or("(none)")
                    ),
                    at_event_index: i,
                }]);
            }
            _ => {}
        }
    }

    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::captured_event::CapturedEvent;
    use chrono::Utc;
    use tempfile::tempdir;

    fn ev(name: &str, ty: Option<&str>) -> CapturedEvent {
        let payload = match ty {
            Some(t) => serde_json::json!({ "type": t }),
            None => serde_json::json!({}),
        };
        CapturedEvent {
            virtual_ts: Utc::now(),
            window: None,
            event_name: name.into(),
            payload,
        }
    }

    #[test]
    fn first_run_records_baseline_and_returns_equivalent() {
        let tmp = tempdir().unwrap();
        let baseline = tmp.path().join("events.baseline.json");
        let captured = vec![ev("continuous_mode_event", Some("started"))];
        let mismatches = compare_events_snapshot(&captured, &baseline).unwrap();
        assert!(mismatches.is_empty());
        assert!(baseline.exists());
    }

    #[test]
    fn second_run_matches_baseline() {
        let tmp = tempdir().unwrap();
        let baseline = tmp.path().join("events.baseline.json");
        let captured = vec![ev("continuous_mode_event", Some("started"))];
        // Record
        compare_events_snapshot(&captured, &baseline).unwrap();
        // Verify
        let mismatches = compare_events_snapshot(&captured, &baseline).unwrap();
        assert!(mismatches.is_empty());
    }

    #[test]
    fn detects_extra_event() {
        let tmp = tempdir().unwrap();
        let baseline = tmp.path().join("events.baseline.json");
        let recorded = vec![ev("continuous_mode_event", Some("started"))];
        compare_events_snapshot(&recorded, &baseline).unwrap();

        let with_extra = vec![
            ev("continuous_mode_event", Some("started")),
            ev("continuous_mode_event", Some("stopped")),
        ];
        let mismatches = compare_events_snapshot(&with_extra, &baseline).unwrap();
        assert_eq!(mismatches.len(), 1);
        assert!(matches!(
            mismatches[0],
            MismatchKind::UnexpectedEvent { .. }
        ));
    }

    #[test]
    fn detects_missing_event() {
        let tmp = tempdir().unwrap();
        let baseline = tmp.path().join("events.baseline.json");
        let recorded = vec![
            ev("continuous_mode_event", Some("started")),
            ev("continuous_mode_event", Some("stopped")),
        ];
        compare_events_snapshot(&recorded, &baseline).unwrap();

        let shortened = vec![ev("continuous_mode_event", Some("started"))];
        let mismatches = compare_events_snapshot(&shortened, &baseline).unwrap();
        assert_eq!(mismatches.len(), 1);
        assert!(matches!(mismatches[0], MismatchKind::MissingEvent { .. }));
    }

    #[test]
    fn detects_reordered_event_via_type_mismatch() {
        let tmp = tempdir().unwrap();
        let baseline = tmp.path().join("events.baseline.json");
        let recorded = vec![
            ev("continuous_mode_event", Some("started")),
            ev("continuous_mode_event", Some("stopped")),
        ];
        compare_events_snapshot(&recorded, &baseline).unwrap();

        // Swap order: stopped before started
        let reordered = vec![
            ev("continuous_mode_event", Some("stopped")),
            ev("continuous_mode_event", Some("started")),
        ];
        let mismatches = compare_events_snapshot(&reordered, &baseline).unwrap();
        assert_eq!(mismatches.len(), 1);
        if let MismatchKind::EventPayload { event_index, field, .. } = &mismatches[0] {
            assert_eq!(*event_index, 0);
            assert_eq!(field, "type");
        } else {
            panic!("expected EventPayload divergence, got {:?}", mismatches[0]);
        }
    }
}
