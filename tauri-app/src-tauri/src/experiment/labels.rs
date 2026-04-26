//! Loader for the labeled corpus at `tests/fixtures/labels/*.json`.
//!
//! Schema (matches `tools/labeled_regression_cli.rs`):
//! ```json
//! {
//!   "session_id": "<full uuid>",
//!   "date": "YYYY-MM-DD",
//!   "labeled_at": "RFC3339",
//!   "labeled_by": "...",
//!   "labels": { LabelData }
//! }
//! ```
//!
//! File naming: `<YYYY-MM-DD>_<short_id>.json` where `short_id` is the first
//! 8 chars of the session UUID.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::Result;

use crate::feedback_to_label::LabelData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEntry {
    pub session_id: String,
    pub date: String,
    #[serde(default)]
    pub labeled_at: Option<String>,
    #[serde(default)]
    pub labeled_by: Option<String>,
    pub labels: LabelData,
}

/// Resolve the labels directory. Defaults to
/// `<crate>/tests/fixtures/labels` when run from `cargo run --bin`.
/// Override via the `AMI_LABELS_DIR` env var (used by tests + custom corpora).
pub fn labels_dir() -> PathBuf {
    if let Ok(p) = std::env::var("AMI_LABELS_DIR") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("labels")
}

/// Load the label for a given session, if one exists. Looks up by
/// `<date>_<short_id>.json` where `short_id` = first 8 chars of `session_id`.
pub fn load_label_for_session(session_id: &str, date: &str) -> Option<LabelEntry> {
    let short = session_id.get(..8)?;
    let path = labels_dir().join(format!("{date}_{short}.json"));
    if !path.exists() {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Walk `labels_dir()` and load every label entry. Used by full-corpus
/// experiment runs.
pub fn load_all_labels() -> Result<Vec<LabelEntry>> {
    let dir = labels_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(label) = serde_json::from_slice::<LabelEntry>(&bytes) {
                out.push(label);
            }
        }
    }
    Ok(out)
}

/// Filter labels matching a date prefix (e.g., "2026-04-24" or "2026-04").
pub fn load_labels_filtered_by_date(date_prefix: &str) -> Result<Vec<LabelEntry>> {
    Ok(load_all_labels()?
        .into_iter()
        .filter(|l| l.date.starts_with(date_prefix))
        .collect())
}

/// Resolve a label corpus location for a CLI. Falls back to a runtime path
/// (`~/.transcriptionapp/labels/`) when the in-tree corpus is unavailable.
pub fn corpus_root_with_runtime_fallback() -> PathBuf {
    let in_tree = labels_dir();
    if in_tree.exists() {
        return in_tree;
    }
    if let Some(home) = dirs::home_dir() {
        let runtime = home.join(".transcriptionapp").join("labels");
        if runtime.exists() {
            return runtime;
        }
    }
    in_tree
}

/// Convenience: a `LabelData` view that distinguishes "label says correct" vs
/// "label was provided". Many fields are `Option<bool>` where `None` means
/// "not labeled".
pub fn label_says_clinical(l: &LabelData) -> Option<bool> {
    l.clinical_correct
}

pub fn label_says_split_correct(l: &LabelData) -> Option<bool> {
    l.split_correct
}

pub fn label_says_billing_codes(l: &LabelData) -> Option<&Vec<String>> {
    l.billing_codes_expected.as_ref()
}

pub fn label_says_diagnostic_code(l: &LabelData) -> Option<&str> {
    l.diagnostic_code_expected.as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_labels_dir_uses_env_override() {
        let dir = tempdir().unwrap();
        std::env::set_var("AMI_LABELS_DIR", dir.path());
        assert_eq!(labels_dir(), dir.path());
        std::env::remove_var("AMI_LABELS_DIR");
    }

    #[test]
    fn test_load_label_for_session_returns_none_when_absent() {
        let dir = tempdir().unwrap();
        std::env::set_var("AMI_LABELS_DIR", dir.path());
        let r = load_label_for_session("00000000-0000-0000-0000-000000000000", "2026-01-01");
        assert!(r.is_none());
        std::env::remove_var("AMI_LABELS_DIR");
    }

    #[test]
    fn test_load_label_for_session_finds_by_short_id_and_date() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("2026-04-24_deb5f823.json");
        let body = r#"{
            "session_id": "deb5f823-1ab7-4e32-87f7-11f9a4cd06fe",
            "date": "2026-04-24",
            "labels": {
                "billing_codes_expected": ["A007A", "Q310A"]
            }
        }"#;
        std::fs::write(&path, body).unwrap();
        std::env::set_var("AMI_LABELS_DIR", dir.path());
        let r = load_label_for_session("deb5f823-1ab7-4e32-87f7-11f9a4cd06fe", "2026-04-24");
        std::env::remove_var("AMI_LABELS_DIR");
        let entry = r.expect("should load");
        assert_eq!(entry.session_id, "deb5f823-1ab7-4e32-87f7-11f9a4cd06fe");
        assert_eq!(
            entry.labels.billing_codes_expected.as_ref().unwrap(),
            &vec!["A007A".to_string(), "Q310A".to_string()]
        );
    }

    #[test]
    fn test_load_all_labels_filters_non_json() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("not-json.txt"), "ignore me").unwrap();
        std::fs::write(
            dir.path().join("2026-04-24_test.json"),
            r#"{"session_id":"x","date":"2026-04-24","labels":{}}"#,
        ).unwrap();
        std::env::set_var("AMI_LABELS_DIR", dir.path());
        let r = load_all_labels().unwrap();
        std::env::remove_var("AMI_LABELS_DIR");
        assert_eq!(r.len(), 1);
    }
}
