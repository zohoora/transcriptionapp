//! Archive-level equivalence comparator.
//!
//! Walks the actual archive (in the test tempdir) against expectations derived
//! from bundle.outcome. Field comparison is allowlist-based — we compare only
//! a known-stable set of metadata fields and ignore timestamps, UUIDs, SOAP
//! content, and any field not listed.
//!
//! First-divergence semantics: returns on the first mismatch found in
//! canonical order. Downstream differences are suppressed by design so the
//! report points at exactly one place behavior diverged.

use super::mismatch_report::MismatchKind;
use crate::replay_bundle::{Outcome, ReplayBundle};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Snapshot of the archive state that the harness produces for a given bundle.
///
/// This is the "baseline" against which future harness runs are compared.
/// Stored as `<bundle_path>.baseline.json` next to the bundle fixture.
///
/// Snapshot semantics: the first harness run on a bundle captures this file
/// (and the test passes). Subsequent runs compare against it. Set
/// HARNESS_RECORD_BASELINES=1 to force re-capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSnapshot {
    /// Number of session directories the orchestrator produced.
    pub session_count: usize,
    /// Per-session summary, ordered by encounter_number (nulls last).
    pub sessions: Vec<SessionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub encounter_number: Option<u64>,
    pub has_soap_note: Option<bool>,
    pub was_merged: Option<bool>,
    pub charting_mode: Option<String>,
    pub detection_method: Option<String>,
    pub likely_non_clinical: Option<bool>,
    pub patient_count: Option<u32>,
}

pub struct ArchiveComparator {
    /// Fields to compare. Ordered — walked in this order for first-divergence
    /// reporting, so fields earlier in this list are surfaced before later ones.
    allowlist: Vec<&'static str>,
}

impl Default for ArchiveComparator {
    fn default() -> Self {
        Self {
            allowlist: vec![
                "encounter_number",
                "has_soap_note",
                "was_merged",
                "merged_into",
                "patient_name",
                "charting_mode",
                "detection_method",
                "likely_non_clinical",
                "patient_count",
            ],
        }
    }
}

impl ArchiveComparator {
    /// Snapshot-based comparison. Given an actual archive root + the path of
    /// a baseline sidecar file, either:
    ///
    /// - If the baseline file doesn't exist or `HARNESS_RECORD_BASELINES=1`:
    ///   record the current actual state as the baseline, return Equivalent.
    ///
    /// - Else: load the baseline, compare against actual, return the first
    ///   divergence or Equivalent.
    pub fn compare_snapshot(
        &self,
        archive_root: &Path,
        baseline_path: &Path,
    ) -> Result<Vec<MismatchKind>, String> {
        let actual_snapshot = snapshot_archive(archive_root);

        let record_mode =
            std::env::var("HARNESS_RECORD_BASELINES").unwrap_or_default() == "1";

        if record_mode || !baseline_path.exists() {
            if let Some(parent) = baseline_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let body = serde_json::to_string_pretty(&actual_snapshot)
                .map_err(|e| e.to_string())?;
            std::fs::write(baseline_path, body).map_err(|e| e.to_string())?;
            return Ok(vec![]);
        }

        let baseline: ArchiveSnapshot = serde_json::from_str(
            &std::fs::read_to_string(baseline_path).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("baseline parse error: {}", e))?;

        if baseline.session_count != actual_snapshot.session_count {
            return Ok(vec![MismatchKind::ArchiveField {
                session: "(aggregate)".into(),
                field: "session_count".into(),
                expected: serde_json::json!(baseline.session_count),
                actual: serde_json::json!(actual_snapshot.session_count),
            }]);
        }

        // Pair sessions by position (already sorted in snapshot_archive).
        for (idx, (exp, act)) in baseline
            .sessions
            .iter()
            .zip(actual_snapshot.sessions.iter())
            .enumerate()
        {
            let session_label = format!(
                "session[{}] (encounter_number={:?})",
                idx, act.encounter_number
            );
            if let Some(diff) = diff_session(&session_label, exp, act) {
                return Ok(vec![diff]);
            }
        }

        Ok(vec![])
    }

    /// Legacy comparison against bundle.outcome. Kept for the unit tests that
    /// exercise the compiled-in matching logic; production harness flow uses
    /// `compare_snapshot`.
    pub fn compare(
        &self,
        bundle: &ReplayBundle,
        archive_root: &Path,
    ) -> Result<Vec<MismatchKind>, String> {
        let outcome = match &bundle.outcome {
            Some(o) => o,
            None => {
                // Bundle has no recorded outcome — nothing to compare against.
                // Treat as trivially equivalent. (This applies to smoke-test
                // bundles that the harness constructs without an outcome.)
                return Ok(vec![]);
            }
        };

        // Session IDs differ between the original production run and the
        // replay: run_continuous_mode generates a fresh UUID each time. We
        // don't match by ID — we find whichever session(s) the orchestrator
        // produced and match by `encounter_number`, which is stable.
        let actual_sessions = all_actual_sessions(archive_root);

        // For was_merged encounters: the bundle's outcome represents a
        // session that was MERGED AWAY during the original run. On replay,
        // depending on when in the lifecycle the bundle was captured, the
        // orchestrator may not reproduce the merge exactly. Skip the strict
        // archive match for merged outcomes — they're better verified by
        // the per-day harness's cross-encounter invariants (Phase 6).
        if outcome.was_merged {
            return Ok(vec![]);
        }

        // Find the actual session with matching encounter_number.
        let matching = actual_sessions.iter().find(|sess| {
            sess.metadata
                .get("encounter_number")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32 == outcome.encounter_number)
                .unwrap_or(false)
        });

        let actual = match matching {
            Some(a) => a,
            None => {
                return Ok(vec![MismatchKind::MissingSession {
                    session: format!(
                        "encounter_number={} (no session in actual archive with that encounter_number; found {})",
                        outcome.encounter_number,
                        actual_sessions.len()
                    ),
                }]);
            }
        };

        let expected_json = expected_metadata_from_outcome(outcome);

        // Walk allowlist fields in order; return first divergence.
        for field in &self.allowlist {
            let expected_val = expected_json
                .get(field)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let actual_val = actual
                .metadata
                .get(field)
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            if !json_equal_ignoring_null(&expected_val, &actual_val) {
                return Ok(vec![MismatchKind::ArchiveField {
                    session: actual.session_id.clone(),
                    field: field.to_string(),
                    expected: expected_val,
                    actual: actual_val,
                }]);
            }
        }

        Ok(vec![])
    }
}

struct ActualSession {
    session_id: String,
    metadata: serde_json::Value,
}

/// Build a snapshot of the actual archive state, suitable for baseline storage.
pub fn snapshot_archive(root: &Path) -> ArchiveSnapshot {
    let sessions = all_actual_sessions(root);
    let mut summaries: Vec<SessionSummary> = sessions
        .iter()
        .map(|s| SessionSummary {
            encounter_number: s.metadata.get("encounter_number").and_then(|v| v.as_u64()),
            has_soap_note: s.metadata.get("has_soap_note").and_then(|v| v.as_bool()),
            was_merged: s.metadata.get("was_merged").and_then(|v| v.as_bool()),
            charting_mode: s
                .metadata
                .get("charting_mode")
                .and_then(|v| v.as_str())
                .map(String::from),
            detection_method: s
                .metadata
                .get("detection_method")
                .and_then(|v| v.as_str())
                .map(String::from),
            likely_non_clinical: s
                .metadata
                .get("likely_non_clinical")
                .and_then(|v| v.as_bool()),
            patient_count: s
                .metadata
                .get("patient_count")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
        })
        .collect();
    // Stable ordering: by encounter_number (nulls last), then by session_id.
    summaries.sort_by(|a, b| match (a.encounter_number, b.encounter_number) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    let session_count = summaries.len();
    ArchiveSnapshot {
        session_count,
        sessions: summaries,
    }
}

fn diff_session(
    session_label: &str,
    expected: &SessionSummary,
    actual: &SessionSummary,
) -> Option<MismatchKind> {
    if expected.encounter_number != actual.encounter_number {
        return Some(MismatchKind::ArchiveField {
            session: session_label.into(),
            field: "encounter_number".into(),
            expected: serde_json::json!(expected.encounter_number),
            actual: serde_json::json!(actual.encounter_number),
        });
    }
    if expected.has_soap_note != actual.has_soap_note {
        return Some(MismatchKind::ArchiveField {
            session: session_label.into(),
            field: "has_soap_note".into(),
            expected: serde_json::json!(expected.has_soap_note),
            actual: serde_json::json!(actual.has_soap_note),
        });
    }
    if expected.was_merged != actual.was_merged {
        return Some(MismatchKind::ArchiveField {
            session: session_label.into(),
            field: "was_merged".into(),
            expected: serde_json::json!(expected.was_merged),
            actual: serde_json::json!(actual.was_merged),
        });
    }
    if expected.detection_method != actual.detection_method {
        return Some(MismatchKind::ArchiveField {
            session: session_label.into(),
            field: "detection_method".into(),
            expected: serde_json::json!(expected.detection_method),
            actual: serde_json::json!(actual.detection_method),
        });
    }
    if expected.likely_non_clinical != actual.likely_non_clinical {
        return Some(MismatchKind::ArchiveField {
            session: session_label.into(),
            field: "likely_non_clinical".into(),
            expected: serde_json::json!(expected.likely_non_clinical),
            actual: serde_json::json!(actual.likely_non_clinical),
        });
    }
    if expected.patient_count != actual.patient_count {
        return Some(MismatchKind::ArchiveField {
            session: session_label.into(),
            field: "patient_count".into(),
            expected: serde_json::json!(expected.patient_count),
            actual: serde_json::json!(actual.patient_count),
        });
    }
    None
}

/// Walk the archive root and return every session directory's metadata.
fn all_actual_sessions(root: &Path) -> Vec<ActualSession> {
    let mut out = Vec::new();
    let year_dirs = match std::fs::read_dir(root) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for y in year_dirs.flatten() {
        if !y.path().is_dir() {
            continue;
        }
        let month_dirs = match std::fs::read_dir(y.path()) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for m in month_dirs.flatten() {
            if !m.path().is_dir() {
                continue;
            }
            let day_dirs = match std::fs::read_dir(m.path()) {
                Ok(r) => r,
                Err(_) => continue,
            };
            for d in day_dirs.flatten() {
                if !d.path().is_dir() {
                    continue;
                }
                let session_dirs = match std::fs::read_dir(d.path()) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                for s in session_dirs.flatten() {
                    if !s.path().is_dir() {
                        continue;
                    }
                    let meta_path = s.path().join("metadata.json");
                    if let Ok(body) = std::fs::read_to_string(&meta_path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                            let sid = s.file_name().to_string_lossy().to_string();
                            out.push(ActualSession {
                                session_id: sid,
                                metadata: json,
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

// Legacy helper retained for potential future use (per-session-id probe).
#[allow(dead_code)]
fn find_session_metadata(root: &Path, session_id: &str) -> Option<PathBuf> {
    // Archive layout: root/YYYY/MM/DD/<session_id>/metadata.json
    // We do a bounded walk (depth 4) rather than a generic recurse.
    let year_dirs = std::fs::read_dir(root).ok()?.flatten();
    for y in year_dirs {
        if !y.path().is_dir() {
            continue;
        }
        let month_dirs = std::fs::read_dir(y.path()).ok()?.flatten();
        for m in month_dirs {
            if !m.path().is_dir() {
                continue;
            }
            let day_dirs = std::fs::read_dir(m.path()).ok()?.flatten();
            for d in day_dirs {
                if !d.path().is_dir() {
                    continue;
                }
                let cand = d.path().join(session_id).join("metadata.json");
                if cand.exists() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

/// Build a JSON object containing only the fields we expect to have been
/// produced, derived from the bundle's recorded outcome.
fn expected_metadata_from_outcome(outcome: &Outcome) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "encounter_number".into(),
        serde_json::json!(outcome.encounter_number),
    );
    // has_soap_note: true if the bundle recorded a successful SOAP outcome.
    // Use outcome.is_clinical as a proxy when soap_result is absent.
    // (Deliberate: non-clinical encounters skip SOAP, so has_soap_note=false.)
    map.insert(
        "has_soap_note".into(),
        serde_json::json!(outcome.is_clinical && !outcome.was_merged),
    );
    map.insert("was_merged".into(), serde_json::json!(outcome.was_merged));
    if let Some(mi) = &outcome.merged_into {
        map.insert("merged_into".into(), serde_json::json!(mi));
    }
    if let Some(name) = &outcome.patient_name {
        map.insert("patient_name".into(), serde_json::json!(name));
    }
    if let Some(method) = &outcome.detection_method {
        map.insert("detection_method".into(), serde_json::json!(method));
    }
    // charting_mode is always "continuous" for orchestrator runs.
    map.insert(
        "charting_mode".into(),
        serde_json::json!("continuous"),
    );
    // likely_non_clinical is set when is_clinical is false.
    if !outcome.is_clinical {
        map.insert(
            "likely_non_clinical".into(),
            serde_json::json!(true),
        );
    }
    serde_json::Value::Object(map)
}

/// JSON equality that treats Null and "missing key" as equivalent on the
/// expected side — we don't penalize fields the bundle didn't explicitly
/// record. Actual can be Null too when the field is genuinely unset.
fn json_equal_ignoring_null(expected: &serde_json::Value, actual: &serde_json::Value) -> bool {
    if expected.is_null() {
        return true; // Nothing expected → any actual is fine.
    }
    expected == actual
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn empty_bundle_with_outcome(outcome: Outcome) -> ReplayBundle {
        ReplayBundle {
            schema_version: 3,
            config: serde_json::json!({}),
            segments: vec![],
            sensor_transitions: vec![],
            vision_results: vec![],
            detection_checks: vec![],
            split_decision: None,
            clinical_check: None,
            merge_check: None,
            soap_result: None,
            billing_result: None,
            multi_patient_detections: vec![],
            name_tracker: None,
            outcome: Some(outcome),
        }
    }

    fn write_session_meta(root: &std::path::Path, session_id: &str, meta: serde_json::Value) {
        let dir = root.join("2026").join("04").join("14").join(session_id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("metadata.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn equivalent_when_metadata_matches_expected_allowlist() {
        let outcome = Outcome {
            session_id: "abc123".into(),
            encounter_number: 1,
            word_count: 500,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            patient_name: Some("Doe, Jane".into()),
            detection_method: Some("llm".into()),
        };
        let bundle = empty_bundle_with_outcome(outcome);
        let root = tempdir().unwrap();
        write_session_meta(
            root.path(),
            "abc123",
            serde_json::json!({
                "session_id": "abc123",
                "encounter_number": 1,
                "has_soap_note": true,
                "was_merged": false,
                "patient_name": "Doe, Jane",
                "charting_mode": "continuous",
                "detection_method": "llm",
            }),
        );

        let cmp = ArchiveComparator::default();
        let mismatches = cmp.compare(&bundle, root.path()).unwrap();
        assert!(mismatches.is_empty(), "expected equivalent, got: {:?}", mismatches);
    }

    #[test]
    fn archive_field_mismatch_returned_with_session_and_field() {
        // Outcome expects has_soap_note=true + patient_name="Jane Doe"; actual
        // session exists with the right encounter_number but different
        // patient_name → ArchiveField divergence on patient_name.
        let outcome = Outcome {
            session_id: "abc123".into(),
            encounter_number: 1,
            word_count: 500,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            patient_name: Some("Jane Doe".into()),
            detection_method: None,
        };
        let bundle = empty_bundle_with_outcome(outcome);
        let root = tempdir().unwrap();
        write_session_meta(
            root.path(),
            "abc123",
            serde_json::json!({
                "encounter_number": 1,
                "has_soap_note": true,
                "was_merged": false,
                "patient_name": "John Smith",
                "charting_mode": "continuous",
            }),
        );

        let cmp = ArchiveComparator::default();
        let mismatches = cmp.compare(&bundle, root.path()).unwrap();
        assert_eq!(mismatches.len(), 1);
        match &mismatches[0] {
            MismatchKind::ArchiveField { session, field, .. } => {
                assert_eq!(session, "abc123");
                assert_eq!(field, "patient_name");
            }
            other => panic!("expected ArchiveField(patient_name), got {:?}", other),
        }
    }

    #[test]
    fn missing_session_reported() {
        let outcome = Outcome {
            session_id: "not_on_disk".into(),
            encounter_number: 1,
            word_count: 0,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            patient_name: None,
            detection_method: None,
        };
        let bundle = empty_bundle_with_outcome(outcome);
        let root = tempdir().unwrap();

        let cmp = ArchiveComparator::default();
        let mismatches = cmp.compare(&bundle, root.path()).unwrap();
        assert_eq!(mismatches.len(), 1);
        assert!(matches!(
            mismatches[0],
            MismatchKind::MissingSession { .. }
        ));
    }

    #[test]
    fn ignores_non_allowlist_fields() {
        let outcome = Outcome {
            session_id: "abc123".into(),
            encounter_number: 1,
            word_count: 500,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            patient_name: None,
            detection_method: None,
        };
        let bundle = empty_bundle_with_outcome(outcome);
        let root = tempdir().unwrap();
        write_session_meta(
            root.path(),
            "abc123",
            serde_json::json!({
                "encounter_number": 1,
                "has_soap_note": true,
                "was_merged": false,
                "charting_mode": "continuous",
                // Not in allowlist — should be ignored:
                "session_id": "abc123",
                "started_at": "9999-01-01T00:00:00Z",
                "duration_ms": 12345,
                "segment_count": 99,
            }),
        );

        let cmp = ArchiveComparator::default();
        let mismatches = cmp.compare(&bundle, root.path()).unwrap();
        assert!(
            mismatches.is_empty(),
            "allowlist should ignore unlisted fields, got: {:?}",
            mismatches
        );
    }

    #[test]
    fn no_outcome_means_trivially_equivalent() {
        let bundle = ReplayBundle {
            schema_version: 3,
            config: serde_json::json!({}),
            segments: vec![],
            sensor_transitions: vec![],
            vision_results: vec![],
            detection_checks: vec![],
            split_decision: None,
            clinical_check: None,
            merge_check: None,
            soap_result: None,
            billing_result: None,
            multi_patient_detections: vec![],
            name_tracker: None,
            outcome: None,
        };
        let root = tempdir().unwrap();
        let cmp = ArchiveComparator::default();
        let mismatches = cmp.compare(&bundle, root.path()).unwrap();
        assert!(mismatches.is_empty());
    }
}
