//! Labeled regression CLI: compares production session outputs to
//! human-reviewed ground truth labels.
//!
//! For each label file in `tests/fixtures/labels/*.json`, finds the corresponding
//! session in `~/.transcriptionapp/archive/` and verifies:
//!   - billing codes match the expected codes
//!   - diagnostic code matches
//!   - SOAP / merge / split decisions are consistent with the label
//!
//! This is the "golden corpus" regression test — when production behavior
//! diverges from a labeled correct answer, it's either a regression in our
//! code or the label needs to be updated.
//!
//! ## Expected-failure baseline (added 2026-05-05)
//!
//! Each label may declare `expected_failures: ["check_name", ...]` listing the
//! checks that are CURRENTLY known to diverge. Those don't count as
//! regressions; only NEW divergences do. Removing an entry tightens the
//! gate — the next run that reproduces that failure will block CI.
//!
//! Bootstrap the baseline against the current archive state with:
//!   cargo run --bin labeled_regression_cli -- --all --bootstrap-expected-failures
//!
//! Stable check names: see `check_names` module below.
//!
//! ## Usage
//!
//!   cargo run --bin labeled_regression_cli -- --all
//!   cargo run --bin labeled_regression_cli -- --all --fail-on-regression
//!   cargo run --bin labeled_regression_cli -- --all --bootstrap-expected-failures
//!   cargo run --bin labeled_regression_cli -- 2026-04-15_00aa31d4.json

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use transcription_app_lib::billing::BillingRecord;
use transcription_app_lib::feedback_to_label::LabelData;
use transcription_app_lib::replay_fetch::ArchiveFetcher;

/// Stable check names emitted by this CLI. Anything that ends up in a label's
/// `expected_failures` array must match one of these (or a per-code variant
/// produced by `billing_codes_unexpected()` / `billing_quantity()`).
mod check_names {
    pub const CLINICAL_CLASSIFICATION: &str = "clinical_classification";
    pub const BILLING_CODES: &str = "billing_codes";
    pub const DIAGNOSTIC_CODE: &str = "diagnostic_code";
    pub const BILLING_JSON_MISSING: &str = "billing_json_missing";
    pub const DATE_PARSE: &str = "date_parse";

    pub fn billing_codes_unexpected(code: &str) -> String {
        format!("billing_codes_unexpected:{code}")
    }
    pub fn billing_quantity(code: &str) -> String {
        format!("billing_quantity:{code}")
    }
}

#[derive(Debug, Deserialize)]
struct Label {
    session_id: String,
    date: String,
    #[allow(dead_code)]
    #[serde(default)]
    labeled_at: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    labeled_by: Option<String>,
    labels: LabelData,
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} [LABEL_FILE | --all] [OPTIONS]", program);
    eprintln!();
    eprintln!("Compare production session outputs to ground truth labels.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  LABEL_FILE          Run a single label file (relative to tests/fixtures/labels/)");
    eprintln!("  --all               Run all labels in tests/fixtures/labels/");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --fail-on-regression           Exit non-zero if any label fails a check that is");
    eprintln!("                                  NOT listed in its `expected_failures` array.");
    eprintln!("  --bootstrap-expected-failures  Rewrite label files: replace `expected_failures`");
    eprintln!("                                  with the names of checks that fail right now.");
    eprintln!("                                  One-time migration to seed the baseline.");
    eprintln!("  --verbose                      Print details for matches as well as mismatches");
    eprintln!("  --help                         Show this help");
}

fn labels_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("labels")
}

fn list_label_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Parse a YYYY-MM-DD label date into a noon-UTC DateTime.
/// Returns None if the date is malformed (rather than silently using today).
fn parse_date(date: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let naive = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let noon = naive.and_hms_opt(12, 0, 0)?;
    Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(noon, chrono::Utc))
}

/// Outcome of running checks against one label.
///
/// Each check produces a stable name and pass/fail. Failures are partitioned
/// into "expected" (in `label.expected_failures`) and "regressions" (everything
/// else). `drift` tracks names that WERE listed as expected but passed in this
/// run — surfaces tightening opportunities.
#[derive(Debug, Default)]
struct CheckResults {
    checks: u32,
    passes: u32,
    /// Unexpected failures, as `(check_name, formatted_message)`. Trigger
    /// `--fail-on-regression`.
    regressions: Vec<(String, String)>,
    /// Expected failures that fired as expected. Counted, not blocking.
    expected_fired: Vec<(String, String)>,
    /// Names listed in `expected_failures` that did NOT fire — corpus drift,
    /// tightening opportunity. Informational only.
    drift: Vec<String>,
}

impl CheckResults {
    fn record_pass(&mut self, name: &str, expected_failures: &[String]) {
        self.checks += 1;
        self.passes += 1;
        if expected_failures.iter().any(|n| n == name) {
            self.drift.push(name.to_string());
        }
    }

    fn record_fail(&mut self, name: &str, msg: String, expected_failures: &[String]) {
        self.checks += 1;
        let entry = (name.to_string(), msg);
        if expected_failures.iter().any(|n| n == name) {
            self.expected_fired.push(entry);
        } else {
            self.regressions.push(entry);
        }
    }

    fn check_eq<T: PartialEq + std::fmt::Debug>(
        &mut self,
        name: &str,
        expected: &T,
        actual: &T,
        expected_failures: &[String],
    ) {
        if expected == actual {
            self.record_pass(name, expected_failures);
        } else {
            let msg = format!("  ✗ {}: expected {:?}, got {:?}", name, expected, actual);
            self.record_fail(name, msg, expected_failures);
        }
    }

    /// Names of checks that failed in this run, regardless of expectation.
    /// Used to seed `expected_failures` during bootstrap.
    fn failed_names(&self) -> BTreeSet<String> {
        self.regressions
            .iter()
            .chain(self.expected_fired.iter())
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn merge_into(&self, totals: &mut Totals) {
        totals.checks += self.checks;
        totals.passes += self.passes;
        totals.regressions += self.regressions.len() as u32;
        totals.expected_fired += self.expected_fired.len() as u32;
        totals.drift += self.drift.len() as u32;
    }

    fn print_report(&self, label_name: &str, label: &Label, verbose: bool) {
        let regressed = !self.regressions.is_empty();
        let interesting = regressed
            || !self.drift.is_empty()
            || !self.expected_fired.is_empty()
            || verbose;
        if !interesting {
            return;
        }
        let label_status = if regressed {
            "REGRESSION"
        } else if !self.expected_fired.is_empty() {
            "expected"
        } else if !self.drift.is_empty() {
            "drift"
        } else {
            "OK"
        };
        println!(
            "{} {} ({} checks, {} pass)",
            label_status, label_name, self.checks, self.passes
        );
        if let Some(notes) = &label.labels.notes {
            println!("  notes: {notes}");
        }
        for (_, msg) in &self.regressions {
            println!("{}", msg);
        }
        for (_, msg) in &self.expected_fired {
            // `≈ (expected)` distinguishes baseline-known failures from real regressions
            // when scrolling the log; without this prefix they'd look identical.
            println!("  ≈ (expected) {}", msg.trim_start());
        }
        for name in &self.drift {
            println!("  ↑ tighten {} (was expected to fail, now passes)", name);
        }
    }
}

#[derive(Debug, Default)]
struct Totals {
    labels: u32,
    checks: u32,
    passes: u32,
    regressions: u32,
    expected_fired: u32,
    drift: u32,
    missing_sessions: u32,
    bootstrapped_files: u32,
}

/// Atomically rewrite a label file with a new `expected_failures` array.
///
/// Round-trips via `serde_json::Value` so we don't accidentally drop fields
/// that the typed `Label` struct doesn't model. Writes via temp-file +
/// rename to avoid leaving a half-written file on crash.
fn rewrite_expected_failures(
    file_path: &Path,
    new_failures: &BTreeSet<String>,
) -> Result<bool, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("read {}: {}", file_path.display(), e))?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse {}: {}", file_path.display(), e))?;

    let labels_obj = value
        .get_mut("labels")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| format!("{}: missing `labels` object", file_path.display()))?;

    let new_array: Vec<serde_json::Value> = new_failures
        .iter()
        .map(|s| serde_json::Value::String(s.clone()))
        .collect();

    let prev = labels_obj.get("expected_failures").cloned();
    let next = if new_array.is_empty() {
        // Drop the field entirely when empty so unbootstrapped files don't grow
        // a noisy `[]`.
        labels_obj.remove("expected_failures");
        serde_json::Value::Null
    } else {
        let arr = serde_json::Value::Array(new_array);
        labels_obj.insert("expected_failures".to_string(), arr.clone());
        arr
    };

    if prev.unwrap_or(serde_json::Value::Null) == next {
        return Ok(false);
    }

    let mut serialized = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("serialize {}: {}", file_path.display(), e))?;
    serialized.push('\n');

    let tmp_name = format!(
        ".{}.{}.tmp",
        file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("label"),
        std::process::id()
    );
    let tmp = file_path.with_file_name(tmp_name);
    fs::write(&tmp, serialized.as_bytes())
        .map_err(|e| format!("write tmp {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, file_path)
        .map_err(|e| format!("rename {} -> {}: {}", tmp.display(), file_path.display(), e))?;
    Ok(true)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];
    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program);
        return if args.contains(&"--help".to_string()) {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    let mut all = false;
    let mut single_file: Option<String> = None;
    let mut fail_on_regression = false;
    let mut bootstrap = false;
    let mut verbose = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => all = true,
            "--fail-on-regression" => fail_on_regression = true,
            "--bootstrap-expected-failures" => bootstrap = true,
            "--verbose" => verbose = true,
            "--help" => {
                print_usage(program);
                return ExitCode::SUCCESS;
            }
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {}", other);
                    return ExitCode::from(1);
                }
                single_file = Some(other.to_string());
            }
        }
        i += 1;
    }

    if bootstrap && fail_on_regression {
        eprintln!(
            "--bootstrap-expected-failures and --fail-on-regression are mutually exclusive: \
             bootstrap rewrites the baseline, so the same run cannot also enforce it."
        );
        return ExitCode::from(1);
    }

    let dir = labels_dir();
    if !dir.exists() {
        eprintln!("Labels directory not found: {}", dir.display());
        return ExitCode::from(1);
    }

    let label_files: Vec<PathBuf> = if all {
        list_label_files(&dir)
    } else if let Some(name) = single_file {
        vec![dir.join(name)]
    } else {
        eprintln!("Provide a label file or --all");
        return ExitCode::from(1);
    };

    if label_files.is_empty() {
        eprintln!("No label files found in {}", dir.display());
        return ExitCode::SUCCESS;
    }

    let fetcher = ArchiveFetcher::from_env().unwrap_or_else(|e| {
        eprintln!("warn: ArchiveFetcher init failed ({e}); falling back to local-only");
        ArchiveFetcher::local_only()
    });

    let mut totals = Totals::default();

    for file in &label_files {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to read {}: {e}", file.display());
                continue;
            }
        };
        let label: Label = match serde_json::from_str(&content) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to parse {}: {e}", file.display());
                continue;
            }
        };

        totals.labels += 1;
        let label_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let expected_failures: Vec<String> =
            label.labels.expected_failures.clone().unwrap_or_default();

        let session = match fetcher.fetch_session(&label.session_id, &label.date).await {
            Ok(s) => s,
            Err(e) => {
                println!("⊘ {} — session not found locally or on server: {e}", label_name);
                totals.missing_sessions += 1;
                continue;
            }
        };

        let mut results = CheckResults::default();

        if let Some(expected) = label.labels.clinical_correct {
            let actual_clinical = !session.metadata.likely_non_clinical.unwrap_or(false);
            results.check_eq(
                check_names::CLINICAL_CLASSIFICATION,
                &expected,
                &actual_clinical,
                &expected_failures,
            );
        }

        let want_billing = label.labels.billing_codes_expected.is_some()
            || label.labels.diagnostic_code_expected.is_some()
            || label.labels.billing_codes_unexpected.is_some()
            || label.labels.billing_quantity_expected.is_some();
        if want_billing {
            match parse_date(&label.date) {
                Some(parsed_date) => {
                    let billing: Option<BillingRecord> = fetcher
                        .fetch_billing(&label.session_id, &parsed_date)
                        .await
                        .unwrap_or(None);
                    check_billing(&mut results, &label.labels, billing.as_ref(), &expected_failures);
                }
                None => {
                    let msg = format!("  ✗ date_parse: cannot parse date {}", label.date);
                    results.record_fail(check_names::DATE_PARSE, msg, &expected_failures);
                }
            }
        }

        // split_correct, merge_correct, patient_count_correct are reserved for
        // future implementation when day_log + replay_bundle inspection lands.

        results.merge_into(&mut totals);
        results.print_report(label_name, &label, verbose);

        if bootstrap {
            match rewrite_expected_failures(file, &results.failed_names()) {
                Ok(true) => totals.bootstrapped_files += 1,
                Ok(false) => {}
                Err(e) => eprintln!("bootstrap failed for {}: {}", file.display(), e),
            }
        }
    }

    println!();
    println!("────────────────────────────────────────────");
    println!(
        "Labels: {}  Checks: {}  Pass: {}  Expected-fail: {}  Regressions: {}  Drift: {}  Missing: {}",
        totals.labels,
        totals.checks,
        totals.passes,
        totals.expected_fired,
        totals.regressions,
        totals.drift,
        totals.missing_sessions
    );
    if bootstrap {
        println!("Bootstrapped {} label files.", totals.bootstrapped_files);
    }

    if fail_on_regression && totals.regressions > 0 {
        eprintln!();
        eprintln!(
            "REGRESSION: {} unexpected check failure(s) across {} label(s). \
             Either fix the code or, if the label needs updating, regenerate the baseline with \
             `--bootstrap-expected-failures`.",
            totals.regressions, totals.labels
        );
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}

fn check_billing(
    results: &mut CheckResults,
    labels: &LabelData,
    billing: Option<&BillingRecord>,
    expected_failures: &[String],
) {
    let Some(billing) = billing else {
        let msg =
            "  ✗ billing_json_missing: billing.json not found, cannot check codes/dx".to_string();
        results.record_fail(check_names::BILLING_JSON_MISSING, msg, expected_failures);
        return;
    };

    let actual_codes: Vec<String> = billing
        .codes
        .iter()
        .map(|c| c.code.clone())
        .chain(billing.time_entries.iter().map(|t| t.code.clone()))
        .collect();

    if let Some(expected_codes) = &labels.billing_codes_expected {
        if expected_codes.iter().all(|c| actual_codes.contains(c)) {
            results.record_pass(check_names::BILLING_CODES, expected_failures);
        } else {
            let mut expected_sorted = expected_codes.clone();
            expected_sorted.sort();
            let mut actual_sorted = actual_codes.clone();
            actual_sorted.sort();
            let msg = format!(
                "  ✗ {}: expected {:?} subset of actual, got {:?}",
                check_names::BILLING_CODES,
                expected_sorted,
                actual_sorted
            );
            results.record_fail(check_names::BILLING_CODES, msg, expected_failures);
        }
    }

    if let Some(unexpected_codes) = &labels.billing_codes_unexpected {
        for code in unexpected_codes {
            let name = check_names::billing_codes_unexpected(code);
            if actual_codes.contains(code) {
                let msg = format!(
                    "  ✗ {}: code {} present (label flagged as inappropriate)",
                    name, code
                );
                results.record_fail(&name, msg, expected_failures);
            } else {
                results.record_pass(&name, expected_failures);
            }
        }
    }

    if let Some(qty_expected) = &labels.billing_quantity_expected {
        for (code, expected_qty) in qty_expected {
            let name = check_names::billing_quantity(code);
            let actual_qty = billing
                .codes
                .iter()
                .find(|c| &c.code == code)
                .map(|c| c.quantity as u32);
            if actual_qty == Some(*expected_qty) {
                results.record_pass(&name, expected_failures);
            } else {
                let msg = format!(
                    "  ✗ {}: expected {}, got {:?}",
                    name, expected_qty, actual_qty
                );
                results.record_fail(&name, msg, expected_failures);
            }
        }
    }

    if let Some(expected_dx) = &labels.diagnostic_code_expected {
        let actual_dx = billing.diagnostic_code.clone().unwrap_or_default();
        let acceptable: Vec<String> = labels.diagnostic_code_acceptable.clone().unwrap_or_default();
        if &actual_dx == expected_dx || acceptable.contains(&actual_dx) {
            results.record_pass(check_names::DIAGNOSTIC_CODE, expected_failures);
        } else {
            let msg = format!(
                "  ✗ {}: expected {} (acceptable: {:?}), got {}",
                check_names::DIAGNOSTIC_CODE,
                expected_dx,
                acceptable,
                actual_dx
            );
            results.record_fail(check_names::DIAGNOSTIC_CODE, msg, expected_failures);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn no_expected() -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn unexpected_failure_is_a_regression() {
        let mut r = CheckResults::default();
        r.check_eq(check_names::BILLING_CODES, &"A".to_string(), &"B".to_string(), &no_expected());
        assert_eq!(r.regressions.len(), 1);
        assert!(r.expected_fired.is_empty());
        assert!(r.drift.is_empty());
        assert!(r.failed_names().contains(check_names::BILLING_CODES));
    }

    #[test]
    fn expected_failure_is_not_a_regression() {
        let mut r = CheckResults::default();
        let expected = vec![check_names::BILLING_CODES.to_string()];
        r.check_eq(check_names::BILLING_CODES, &"A".to_string(), &"B".to_string(), &expected);
        assert!(r.regressions.is_empty());
        assert_eq!(r.expected_fired.len(), 1);
        assert!(r.drift.is_empty());
    }

    #[test]
    fn expected_pass_surfaces_drift() {
        let mut r = CheckResults::default();
        let expected = vec![check_names::BILLING_CODES.to_string()];
        r.check_eq(check_names::BILLING_CODES, &"A".to_string(), &"A".to_string(), &expected);
        assert_eq!(r.passes, 1);
        assert_eq!(r.drift, vec![check_names::BILLING_CODES.to_string()]);
        assert!(r.regressions.is_empty());
        assert!(r.expected_fired.is_empty());
    }

    #[test]
    fn unexpected_pass_does_not_surface_drift() {
        let mut r = CheckResults::default();
        r.check_eq(check_names::BILLING_CODES, &"A".to_string(), &"A".to_string(), &no_expected());
        assert_eq!(r.passes, 1);
        assert!(r.drift.is_empty());
    }

    #[test]
    fn record_fail_with_expected_name_is_expected() {
        let mut r = CheckResults::default();
        let name = check_names::billing_codes_unexpected("K005A");
        let expected = vec![name.clone()];
        r.record_fail(&name, format!("  ✗ {}: code K005A present", name), &expected);
        assert_eq!(r.expected_fired.len(), 1);
        assert!(r.regressions.is_empty());
    }

    #[test]
    fn record_fail_unexpected_name_is_a_regression() {
        let mut r = CheckResults::default();
        let expected = vec![check_names::billing_codes_unexpected("OTHER")];
        let name = check_names::billing_codes_unexpected("K005A");
        r.record_fail(&name, format!("  ✗ {}: code K005A present", name), &expected);
        assert!(r.expected_fired.is_empty());
        assert_eq!(r.regressions.len(), 1);
    }

    #[test]
    fn failed_names_aggregates_across_partitions() {
        let mut r = CheckResults::default();
        let expected = vec![check_names::DIAGNOSTIC_CODE.to_string()];
        r.check_eq(check_names::BILLING_CODES, &"A".to_string(), &"B".to_string(), &expected);
        r.check_eq(check_names::DIAGNOSTIC_CODE, &"311".to_string(), &"401".to_string(), &expected);
        let names = r.failed_names();
        assert!(names.contains(check_names::BILLING_CODES));
        assert!(names.contains(check_names::DIAGNOSTIC_CODE));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn rewrite_expected_failures_writes_array_when_failures_exist() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("label.json");
        fs::write(
            &path,
            r#"{
  "session_id": "abc",
  "date": "2026-04-30",
  "labels": {
    "billing_codes_expected": ["A007A"]
  }
}
"#,
        )
        .unwrap();

        let mut failures = BTreeSet::new();
        failures.insert(check_names::BILLING_CODES.to_string());
        failures.insert(check_names::DIAGNOSTIC_CODE.to_string());

        let changed = rewrite_expected_failures(&path, &failures).unwrap();
        assert!(changed);

        let content = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let arr = parsed["labels"]["expected_failures"].as_array().unwrap();
        let names: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, vec![check_names::BILLING_CODES, check_names::DIAGNOSTIC_CODE]);
    }

    #[test]
    fn rewrite_expected_failures_drops_field_when_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("label.json");
        fs::write(
            &path,
            r#"{
  "session_id": "abc",
  "date": "2026-04-30",
  "labels": {
    "billing_codes_expected": ["A007A"],
    "expected_failures": ["billing_codes"]
  }
}
"#,
        )
        .unwrap();

        let failures = BTreeSet::new();
        let changed = rewrite_expected_failures(&path, &failures).unwrap();
        assert!(changed);

        let content = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["labels"].get("expected_failures").is_none());
    }

    #[test]
    fn rewrite_expected_failures_is_idempotent_when_unchanged() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("label.json");
        fs::write(
            &path,
            r#"{
  "session_id": "abc",
  "date": "2026-04-30",
  "labels": {
    "billing_codes_expected": ["A007A"],
    "expected_failures": ["billing_codes", "diagnostic_code"]
  }
}
"#,
        )
        .unwrap();

        let mut failures = BTreeSet::new();
        failures.insert(check_names::BILLING_CODES.to_string());
        failures.insert(check_names::DIAGNOSTIC_CODE.to_string());

        let changed = rewrite_expected_failures(&path, &failures).unwrap();
        assert!(!changed, "no diff expected when failures match the existing array");
    }

    #[test]
    fn rewrite_expected_failures_preserves_other_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("label.json");
        fs::write(
            &path,
            r#"{
  "session_id": "abc",
  "date": "2026-04-30",
  "labeled_at": "2026-05-01T12:00:00Z",
  "labeled_by": "Dr Z",
  "extra_top_level": "preserve me",
  "labels": {
    "billing_codes_expected": ["A007A"],
    "diagnostic_code_expected": "311",
    "notes": "Anxiety encounter",
    "extra_label_field": 42
  }
}
"#,
        )
        .unwrap();

        let mut failures = BTreeSet::new();
        failures.insert(check_names::BILLING_CODES.to_string());

        rewrite_expected_failures(&path, &failures).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["labeled_by"], "Dr Z");
        assert_eq!(parsed["extra_top_level"], "preserve me");
        assert_eq!(parsed["labels"]["notes"], "Anxiety encounter");
        assert_eq!(parsed["labels"]["extra_label_field"], 42);
        assert_eq!(parsed["labels"]["billing_codes_expected"][0], "A007A");
        assert_eq!(parsed["labels"]["diagnostic_code_expected"], "311");
    }

    #[test]
    fn parse_date_accepts_valid_ymd() {
        assert!(parse_date("2026-04-30").is_some());
    }

    #[test]
    fn parse_date_rejects_garbage() {
        assert!(parse_date("not-a-date").is_none());
        assert!(parse_date("2026-13-01").is_none());
        assert!(parse_date("").is_none());
    }
}
