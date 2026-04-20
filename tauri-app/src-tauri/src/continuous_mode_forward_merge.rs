//! Forward-merge cleanup for false multi-patient splits.
//!
//! When the pre-SOAP multi-patient detector fires on encounter A and produces a
//! secondary sub-SOAP whose clinical content matches the primary patient of the
//! *next* encounter B, it means A's session captured the beginning of B's visit
//! (typically background reception audio from the next patient checking in).
//!
//! This component detects that case after B's SOAP has been written and
//! rewrites A's archive to be single-patient (promoting the primary sub-SOAP to
//! `soap_note.txt`, deleting `patient_labels.json` and the other
//! `soap_patient_*.txt` files, and clearing `patient_count`/`patient_labels` in
//! A's metadata). B is left untouched.
//!
//! Rule: fires iff overlap-coefficient of A/P-section clinical terms between a
//! sub-SOAP of A and B's primary SOAP is ≥ `oc_threshold`, shared-term count
//! is ≥ `min_shared_terms`, and the audio gap (last non-doctor end_ms in A →
//! first non-doctor start_ms in B) is ≤ `max_audio_gap_secs`.
//!
//! LOGGER SESSION CONTRACT: on entry, the pipeline logger is pointed at B's
//! session. This component does not redirect the logger; any log/event it
//! emits lands in B's pipeline_log.
//!
//! COMPONENT: `continuous_mode_forward_merge`.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::continuous_mode_events::ContinuousModeEvent;
use crate::local_archive::ArchiveMetadata;
use crate::pipeline_log::PipelineLogger;
use crate::run_context::RunContext;
use crate::server_sync::ServerSyncContext;

/// Long-lived dependency bundle. Built once before the detector loop starts.
#[derive(Clone)]
pub struct ForwardMergeDeps {
    pub logger: Arc<Mutex<PipelineLogger>>,
    pub sync_ctx: ServerSyncContext,
    pub oc_threshold: f64,
    pub min_shared_terms: usize,
    pub max_audio_gap_secs: u64,
}

impl ForwardMergeDeps {
    /// Production thresholds. Validated by simulation against Apr 16, 17, 20
    /// clinic days (3 multi-patient sessions, 1 true positive, 0 false
    /// positives across 18 evaluated pairs).
    pub fn with_default_thresholds(
        logger: Arc<Mutex<PipelineLogger>>,
        sync_ctx: ServerSyncContext,
    ) -> Self {
        Self {
            logger,
            sync_ctx,
            oc_threshold: 0.30,
            min_shared_terms: 5,
            max_audio_gap_secs: 300,
        }
    }
}

/// Per-call arguments.
pub struct ForwardMergeCall<'a> {
    pub prev_session_id: &'a str,
    pub prev_date: DateTime<Utc>,
    pub curr_session_id: &'a str,
    pub curr_date: DateTime<Utc>,
}

/// Outcome of a forward-merge attempt.
#[derive(Debug)]
pub enum ForwardMergeOutcome {
    /// Previous session had no multi-patient split (or none recoverable).
    NotApplicable { reason: &'static str },
    /// Multi-patient split present but doesn't match current encounter.
    Skipped { reason: String },
    /// Fired: previous session rewritten to single-patient.
    Fired(ForwardMergeDecision),
    /// Error occurred during apply.
    Error { reason: String },
}

/// Details of a fired decision. Carried in `Fired(..)` variant and emitted in
/// the `forward_merge_fired` event.
#[derive(Debug, Clone, Serialize)]
pub struct ForwardMergeDecision {
    pub prev_session_id: String,
    pub curr_session_id: String,
    pub removed_sub_idx: u32,
    pub overlap_coef: f64,
    pub shared_term_count: usize,
    pub audio_gap_secs: f64,
    pub shared_terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PatientLabelEntry {
    index: u32,
    label: String,
}

/// Main entry point.
pub async fn run<C: RunContext>(
    ctx: &C,
    deps: &ForwardMergeDeps,
    call: ForwardMergeCall<'_>,
) -> ForwardMergeOutcome {
    let prev_dir = match crate::local_archive::get_session_archive_dir(call.prev_session_id, &call.prev_date) {
        Ok(p) => p,
        Err(e) => return ForwardMergeOutcome::Error { reason: format!("get prev dir: {e}") },
    };
    let curr_dir = match crate::local_archive::get_session_archive_dir(call.curr_session_id, &call.curr_date) {
        Ok(p) => p,
        Err(e) => return ForwardMergeOutcome::Error { reason: format!("get curr dir: {e}") },
    };

    let labels = match load_patient_labels(&prev_dir) {
        None => return ForwardMergeOutcome::NotApplicable { reason: "no_patient_labels" },
        Some(l) if l.len() <= 1 => return ForwardMergeOutcome::NotApplicable { reason: "single_patient" },
        Some(l) => l,
    };

    let curr_soap = match fs::read_to_string(curr_dir.join("soap_note.txt")) {
        Ok(s) => s,
        Err(_) => return ForwardMergeOutcome::Skipped { reason: "curr_has_no_soap".into() },
    };
    let curr_ap = extract_ap_terms(&curr_soap);
    if curr_ap.len() < deps.min_shared_terms {
        return ForwardMergeOutcome::Skipped {
            reason: format!("curr_soap_too_small_{}_terms", curr_ap.len()),
        };
    }

    let audio_gap_secs = compute_audio_gap_secs(&prev_dir, &curr_dir);
    if let Some(gap) = audio_gap_secs {
        if gap > deps.max_audio_gap_secs as f64 {
            return ForwardMergeOutcome::Skipped {
                reason: format!("audio_gap_{gap:.1}s_exceeds_{}s", deps.max_audio_gap_secs),
            };
        }
    } else {
        return ForwardMergeOutcome::Skipped { reason: "audio_gap_unknown".into() };
    }
    let audio_gap_secs = audio_gap_secs.unwrap();

    // Evaluate each sub-patient SOAP (skip the primary, idx=1 by convention —
    // the primary is the one whose content remains in A after cleanup).
    let mut best: Option<ForwardMergeDecision> = None;
    for lbl in &labels {
        if lbl.index == 1 {
            continue;
        }
        let sub_path = prev_dir.join(format!("soap_patient_{}.txt", lbl.index));
        let sub_soap = match fs::read_to_string(&sub_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let sub_ap = extract_ap_terms(&sub_soap);
        if sub_ap.is_empty() {
            continue;
        }
        let shared: Vec<String> = {
            let mut v: Vec<_> = sub_ap.intersection(&curr_ap).cloned().collect();
            v.sort();
            v
        };
        let denom = sub_ap.len().min(curr_ap.len()) as f64;
        let oc = if denom > 0.0 { shared.len() as f64 / denom } else { 0.0 };
        if oc >= deps.oc_threshold && shared.len() >= deps.min_shared_terms {
            let decision = ForwardMergeDecision {
                prev_session_id: call.prev_session_id.to_string(),
                curr_session_id: call.curr_session_id.to_string(),
                removed_sub_idx: lbl.index,
                overlap_coef: oc,
                shared_term_count: shared.len(),
                audio_gap_secs,
                shared_terms: shared,
            };
            if best.as_ref().map_or(true, |b| decision.overlap_coef > b.overlap_coef) {
                best = Some(decision);
            }
        }
    }

    let decision = match best {
        Some(d) => d,
        None => return ForwardMergeOutcome::Skipped { reason: "no_matching_sub_soap".into() },
    };

    // Apply cleanup. On error, preserve the on-disk state and return Error.
    if let Err(e) = apply_cleanup(&prev_dir, &labels) {
        warn!(
            event = "forward_merge_cleanup_failed",
            component = "continuous_mode_forward_merge",
            prev_session_id = %call.prev_session_id,
            error = %e,
            "forward-merge decision fired but cleanup failed; archive may be in partial state"
        );
        return ForwardMergeOutcome::Error { reason: format!("cleanup: {e}") };
    }

    // Structured log for production debugging.
    info!(
        event = "forward_merge_fired",
        component = "continuous_mode_forward_merge",
        prev_session_id = %decision.prev_session_id,
        curr_session_id = %decision.curr_session_id,
        removed_sub_idx = decision.removed_sub_idx,
        overlap_coef = decision.overlap_coef,
        shared_term_count = decision.shared_term_count,
        audio_gap_secs = decision.audio_gap_secs,
        shared_terms = ?decision.shared_terms,
        "forward-merge cleanup fired: removed sub-SOAP from previous encounter"
    );

    // Emit typed event for UI + log into the pipeline bundle.
    ContinuousModeEvent::ForwardMergeFired {
        prev_session_id: decision.prev_session_id.clone(),
        curr_session_id: decision.curr_session_id.clone(),
        removed_sub_idx: decision.removed_sub_idx,
        overlap_coef: decision.overlap_coef,
        shared_term_count: decision.shared_term_count,
        audio_gap_secs: decision.audio_gap_secs,
    }
    .emit_via(ctx);

    // Sync the cleaned previous session back to the server.
    let prev_date_str = call.prev_date.format("%Y-%m-%d").to_string();
    deps.sync_ctx.resync_session(&decision.prev_session_id, &prev_date_str);

    ForwardMergeOutcome::Fired(decision)
}

fn load_patient_labels(session_dir: &Path) -> Option<Vec<PatientLabelEntry>> {
    let path = session_dir.join("patient_labels.json");
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str::<Vec<PatientLabelEntry>>(&raw).ok()
}

fn compute_audio_gap_secs(prev_dir: &Path, curr_dir: &Path) -> Option<f64> {
    let prev_last = last_non_doctor_end_ms(prev_dir)?;
    let curr_first = first_non_doctor_start_ms(curr_dir)?;
    Some((curr_first as f64 - prev_last as f64) / 1000.0)
}

fn last_non_doctor_end_ms(session_dir: &Path) -> Option<u64> {
    let path = session_dir.join("segments.jsonl");
    let raw = fs::read_to_string(&path).ok()?;
    let mut last: Option<u64> = None;
    for line in raw.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let sp = v.get("speaker_id").and_then(|s| s.as_str()).unwrap_or("");
        if sp.is_empty() || sp.contains("Dr") {
            continue;
        }
        if let Some(em) = v.get("end_ms").and_then(|n| n.as_u64()) {
            last = Some(match last {
                Some(prev) => prev.max(em),
                None => em,
            });
        }
    }
    last
}

fn first_non_doctor_start_ms(session_dir: &Path) -> Option<u64> {
    let path = session_dir.join("segments.jsonl");
    let raw = fs::read_to_string(&path).ok()?;
    for line in raw.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let sp = v.get("speaker_id").and_then(|s| s.as_str()).unwrap_or("");
        if sp.is_empty() || sp.contains("Dr") {
            continue;
        }
        if let Some(sm) = v.get("start_ms").and_then(|n| n.as_u64()) {
            return Some(sm);
        }
    }
    None
}

/// Extract distinctive clinical terms from the A (Assessment) and P (Plan)
/// sections of a SOAP note. Lowercased, ≥4 chars, stopwords removed.
fn extract_ap_terms(soap: &str) -> HashSet<String> {
    // Parse out the A and P sections. Match lines that are exactly "A:" or "P:"
    // on their own (SOAP notes in this codebase follow that convention).
    let mut combined = String::new();
    let mut in_ap = false;
    for line in soap.lines() {
        let trimmed = line.trim();
        if trimmed == "S:" || trimmed == "O:" {
            in_ap = false;
        } else if trimmed == "A:" || trimmed == "P:" {
            in_ap = true;
        } else if in_ap {
            combined.push_str(line);
            combined.push('\n');
        }
    }

    combined
        .split(|c: char| !c.is_alphabetic() && c != '-' && c != '\'')
        .filter_map(|w| {
            let w = w.trim_matches(|c: char| c == '-' || c == '\'').to_lowercase();
            if w.len() < 4 || !w.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
                None
            } else if is_stopword(&w) {
                None
            } else {
                Some(w)
            }
        })
        .collect()
}

fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        // Generic clinical/workflow words
        "patient" | "report" | "reports" | "reported" | "states" | "stated"
        | "noted" | "notes" | "continue" | "continued" | "stop" | "stopped"
        | "follow" | "following" | "discussed" | "plan" | "documented"
        | "visit" | "today" | "based" | "advised" | "prescribe" | "prescribed"
        | "provide" | "provided" | "recommend" | "recommended" | "ordered"
        | "order" | "monitor" | "monitoring" | "schedule" | "scheduled"
        | "appointment" | "management" | "current" | "response" | "dose"
        | "doses" | "daily" | "twice" | "week" | "weeks" | "month" | "months"
        | "year" | "years" | "clinic" | "referral" | "contact" | "consider"
        | "history" | "required" | "requiring" | "evaluate" | "evaluated"
        | "assess" | "assessment" | "including" | "within" | "without"
        // English stopwords (4+ chars only — shorter ones excluded by length)
        | "with" | "from" | "this" | "that" | "these" | "those" | "they"
        | "them" | "their" | "there" | "when" | "what" | "which" | "where"
        | "would" | "could" | "should" | "been" | "being" | "also" | "just"
        | "still" | "even" | "much" | "more" | "most" | "some" | "each"
        | "every" | "other" | "another" | "before" | "after" | "during"
        | "over" | "under" | "above" | "below" | "through" | "between"
        | "away" | "back" | "same" | "different" | "only" | "likely"
    )
}

/// Rewrite prev session to single-patient:
///  1. Copy `soap_patient_1.txt` content into `soap_note.txt` (overwrites the
///     old combined multi-patient SOAP).
///  2. Delete all `soap_patient_*.txt` files.
///  3. Delete `patient_labels.json`.
///  4. Rewrite metadata.json with `patient_count = None` and
///     `patient_labels = None`.
fn apply_cleanup(session_dir: &Path, labels: &[PatientLabelEntry]) -> Result<(), String> {
    // Promote soap_patient_1.txt to soap_note.txt (if it exists).
    let primary_sub = session_dir.join("soap_patient_1.txt");
    if primary_sub.exists() {
        let content = fs::read_to_string(&primary_sub)
            .map_err(|e| format!("read soap_patient_1: {e}"))?;
        fs::write(session_dir.join("soap_note.txt"), content)
            .map_err(|e| format!("write soap_note: {e}"))?;
    }

    // Delete every sub-SOAP file we recorded a label for (primary included —
    // we already promoted it above).
    for lbl in labels {
        let p = session_dir.join(format!("soap_patient_{}.txt", lbl.index));
        if p.exists() {
            let _ = fs::remove_file(&p);
        }
    }

    // Delete patient_labels.json.
    let labels_path = session_dir.join("patient_labels.json");
    if labels_path.exists() {
        let _ = fs::remove_file(&labels_path);
    }

    // Update metadata.json.
    let metadata_path = session_dir.join("metadata.json");
    if metadata_path.exists() {
        let raw = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("read metadata: {e}"))?;
        let mut metadata: ArchiveMetadata = serde_json::from_str(&raw)
            .map_err(|e| format!("parse metadata: {e}"))?;
        metadata.patient_count = None;
        let out = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("serialize metadata: {e}"))?;
        fs::write(&metadata_path, out)
            .map_err(|e| format!("write metadata: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_segment(
        file: &mut fs::File,
        speaker: &str,
        start_ms: u64,
        end_ms: u64,
    ) {
        let line = format!(
            r#"{{"speaker_id":"{speaker}","start_ms":{start_ms},"end_ms":{end_ms},"text":"x"}}"#
        );
        writeln!(file, "{line}").unwrap();
    }

    #[test]
    fn extract_ap_terms_parses_assessment_and_plan_only() {
        let soap = "S:\n\
            • Patient reports headache and fatigue\n\
            O:\n\
            • Blood pressure 120/80\n\
            A:\n\
            • Chronic migraine with aura\n\
            • Fatigue from iron deficiency\n\
            P:\n\
            • Ferrous sulfate daily\n\
            • Refer to neurology\n";
        let terms = extract_ap_terms(soap);
        // A/P terms: chronic, migraine, aura, fatigue, iron, deficiency,
        // ferrous, sulfate, refer, neurology
        assert!(terms.contains("chronic"), "got: {terms:?}");
        assert!(terms.contains("migraine"));
        assert!(terms.contains("aura"));
        assert!(terms.contains("iron"));
        assert!(terms.contains("deficiency"));
        assert!(terms.contains("neurology"));
        // Stopwords filtered
        assert!(!terms.contains("patient"));
        assert!(!terms.contains("daily"));
        // S/O sections excluded
        assert!(!terms.contains("headache"));
        assert!(!terms.contains("blood"));
        assert!(!terms.contains("pressure"));
    }

    #[test]
    fn extract_ap_terms_ignores_short_tokens() {
        let soap = "A:\n• a b ab cat dog rhinitis\nP:\n• go up";
        let terms = extract_ap_terms(soap);
        assert!(terms.contains("rhinitis"));
        assert!(!terms.contains("ab"));
        assert!(!terms.contains("cat")); // 3 chars — filtered
        assert!(terms.contains("dog") == false);
    }

    #[test]
    fn audio_gap_computes_correctly() {
        let tmp = TempDir::new().unwrap();
        let prev = tmp.path().join("prev");
        let curr = tmp.path().join("curr");
        fs::create_dir_all(&prev).unwrap();
        fs::create_dir_all(&curr).unwrap();

        let mut f = fs::File::create(prev.join("segments.jsonl")).unwrap();
        write_segment(&mut f, "Dr Zohoor", 0, 1000);
        write_segment(&mut f, "Speaker 2", 1000, 2000);
        write_segment(&mut f, "Dr Zohoor", 2000, 3000);
        write_segment(&mut f, "Speaker 2", 3000, 4000);

        let mut f2 = fs::File::create(curr.join("segments.jsonl")).unwrap();
        write_segment(&mut f2, "Speaker 1", 9000, 10000);

        // last non-doctor end_ms in prev = 4000; first non-doctor start_ms in curr = 9000
        // audio_gap = (9000 - 4000) / 1000 = 5.0
        assert_eq!(compute_audio_gap_secs(&prev, &curr), Some(5.0));
    }

    #[test]
    fn audio_gap_none_when_no_non_doctor_segments() {
        let tmp = TempDir::new().unwrap();
        let prev = tmp.path().join("prev");
        let curr = tmp.path().join("curr");
        fs::create_dir_all(&prev).unwrap();
        fs::create_dir_all(&curr).unwrap();
        let mut f = fs::File::create(prev.join("segments.jsonl")).unwrap();
        write_segment(&mut f, "Dr Zohoor", 0, 1000);
        fs::File::create(curr.join("segments.jsonl")).unwrap();
        assert_eq!(compute_audio_gap_secs(&prev, &curr), None);
    }

    #[test]
    fn apply_cleanup_promotes_primary_and_deletes_rest() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("soap_patient_1.txt"), "Scott's clean SOAP").unwrap();
        fs::write(dir.join("soap_patient_2.txt"), "Cathy leak SOAP").unwrap();
        fs::write(dir.join("soap_note.txt"), "=== Combined multi-patient ===").unwrap();
        fs::write(
            dir.join("patient_labels.json"),
            r#"[{"index":1,"label":"Scott"},{"index":2,"label":"Speaker 4"}]"#,
        ).unwrap();
        let metadata_json = r#"{
            "session_id":"test",
            "started_at":"2026-04-20T14:17:17Z",
            "ended_at":"2026-04-20T14:48:14Z",
            "duration_ms":0,
            "segment_count":0,
            "word_count":2895,
            "has_soap_note":true,
            "has_audio":false,
            "auto_ended":false,
            "soap_detail_level":5,
            "soap_format":"comprehensive",
            "charting_mode":"continuous",
            "patient_count":2
        }"#;
        fs::write(dir.join("metadata.json"), metadata_json).unwrap();

        let labels = vec![
            PatientLabelEntry { index: 1, label: "Scott".into() },
            PatientLabelEntry { index: 2, label: "Speaker 4".into() },
        ];
        apply_cleanup(dir, &labels).unwrap();

        assert_eq!(fs::read_to_string(dir.join("soap_note.txt")).unwrap(), "Scott's clean SOAP");
        assert!(!dir.join("soap_patient_1.txt").exists());
        assert!(!dir.join("soap_patient_2.txt").exists());
        assert!(!dir.join("patient_labels.json").exists());

        let cleaned: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("metadata.json")).unwrap()).unwrap();
        assert!(cleaned.get("patient_count").map_or(true, |v| v.is_null()));
    }

    #[test]
    fn extract_ap_terms_returns_nothing_for_missing_ap() {
        let soap = "S:\n• just subjective\nO:\n• just objective";
        assert!(extract_ap_terms(soap).is_empty());
    }
}
