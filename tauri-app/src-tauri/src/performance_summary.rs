//! Day-level performance summary for continuous-mode operational analysis.
//!
//! Aggregates data from the day's per-session `pipeline_log.jsonl` files and
//! the `day_log.jsonl` into a single `performance_summary.json` at the day
//! directory. Makes it trivial to answer questions like "what were p90
//! latencies across the day?" without manually parsing 10+ files.
//!
//! Rationale: the Apr 17 2026 ops audit established that latency bottlenecks
//! are quantifiable from `pipeline_log` alone but require laborious per-session
//! aggregation to compute. This module runs once per `continuous_mode_stopped`
//! event and produces a precomputed summary that tools (CLI, dashboards,
//! future Rust tests) can load directly.
//!
//! The summary is additive — later stops of the same day overwrite the file
//! with the updated aggregate. On a 2-run day (morning + evening continuous
//! mode) the evening stop's summary reflects both runs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Per-step latency + concurrency aggregates across the day.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepStats {
    pub step: String,
    pub count: usize,
    pub failure_count: usize,
    /// Cumulative wall-clock latency across all calls, milliseconds.
    pub total_wall_ms: u64,
    /// p50 / p90 / p99 / max of per-call `latency_ms` (wall-clock).
    pub latency_p50_ms: u64,
    pub latency_p90_ms: u64,
    pub latency_p99_ms: u64,
    pub latency_max_ms: u64,
    /// Cumulative time attributed to app-side scheduling (from `CallMetrics`).
    /// None when the step's call sites haven't been migrated to `generate_timed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_scheduling_ms: Option<u64>,
    /// Cumulative time attributed to LLM + network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_network_ms: Option<u64>,
    /// Peak concurrent-call count observed at any call's start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_concurrent: Option<usize>,
    /// Number of calls that triggered a retry (indicates 5xx / connection errors).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retried_call_count: Option<usize>,
}

/// Day-level aggregate across all sessions + the shared `day_log.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySummary {
    pub date: String,
    pub generated_at: String,
    pub session_count: usize,
    pub splits: usize,
    pub merges: usize,
    pub flushes: usize,
    /// Per-step LLM aggregates sorted by total_wall_ms desc.
    pub per_step: Vec<StepStats>,
    /// Total LLM wait across all steps (sum of total_wall_ms).
    pub total_llm_wall_ms: u64,
}

/// Compute and persist the summary for the given date directory. Safe to call
/// repeatedly — overwrites the prior `performance_summary.json` atomically.
pub fn write_day_summary(date_dir: &Path, date: &str) {
    let summary = match compute_summary(date_dir, date) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to compute day summary for {}: {}", date, e);
            return;
        }
    };
    let path = date_dir.join("performance_summary.json");
    let json = match serde_json::to_string_pretty(&summary) {
        Ok(j) => j,
        Err(e) => {
            warn!("Failed to serialize day summary: {}", e);
            return;
        }
    };
    // Atomic write via temp + rename
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = fs::write(&tmp, json) {
        warn!("Failed to write day summary temp: {}", e);
        return;
    }
    if let Err(e) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        warn!("Failed to rename day summary: {}", e);
        return;
    }
    info!(
        "Wrote day summary to {} ({} sessions, {}ms total LLM wait)",
        path.display(),
        summary.session_count,
        summary.total_llm_wall_ms
    );
}

fn compute_summary(date_dir: &Path, date: &str) -> Result<DaySummary, String> {
    if !date_dir.is_dir() {
        return Err(format!("Not a directory: {}", date_dir.display()));
    }

    // Per-step accumulators
    let mut per_step_latencies: HashMap<String, Vec<u64>> = HashMap::new();
    let mut per_step_failures: HashMap<String, usize> = HashMap::new();
    let mut per_step_scheduling: HashMap<String, u64> = HashMap::new();
    let mut per_step_network: HashMap<String, u64> = HashMap::new();
    let mut per_step_peak_concurrent: HashMap<String, usize> = HashMap::new();
    let mut per_step_retried: HashMap<String, usize> = HashMap::new();
    let mut per_step_has_metrics: HashMap<String, bool> = HashMap::new();
    let mut session_count = 0usize;

    // Walk each session directory, parsing its pipeline_log.jsonl.
    for entry in fs::read_dir(date_dir).map_err(|e| e.to_string())? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let log_path = path.join("pipeline_log.jsonl");
        if !log_path.exists() {
            continue;
        }
        session_count += 1;
        let content = match fs::read_to_string(&log_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for line in content.lines() {
            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let step = match entry.get("step").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let latency = match entry.get("latency_ms").and_then(|v| v.as_u64()) {
                Some(n) => n,
                None => continue,
            };
            let success = entry.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
            per_step_latencies.entry(step.clone()).or_default().push(latency);
            if !success {
                *per_step_failures.entry(step.clone()).or_default() += 1;
            }
            if let Some(ctx) = entry.get("context").and_then(|v| v.as_object()) {
                let mut saw_metric = false;
                if let Some(s) = ctx.get("scheduling_ms").and_then(|v| v.as_u64()) {
                    *per_step_scheduling.entry(step.clone()).or_default() += s;
                    saw_metric = true;
                }
                if let Some(n) = ctx.get("network_ms").and_then(|v| v.as_u64()) {
                    *per_step_network.entry(step.clone()).or_default() += n;
                }
                if let Some(c) = ctx.get("concurrent_at_start").and_then(|v| v.as_u64()) {
                    let c = c as usize;
                    let entry = per_step_peak_concurrent.entry(step.clone()).or_default();
                    if c > *entry {
                        *entry = c;
                    }
                }
                if let Some(r) = ctx.get("retry_count").and_then(|v| v.as_u64()) {
                    if r > 0 {
                        *per_step_retried.entry(step.clone()).or_default() += 1;
                    }
                }
                if saw_metric {
                    per_step_has_metrics.insert(step, true);
                }
            }
        }
    }

    // Build per-step stats with percentiles.
    let mut per_step: Vec<StepStats> = per_step_latencies
        .into_iter()
        .map(|(step, mut lats)| {
            lats.sort_unstable();
            let n = lats.len();
            let percentile = |p: f64| -> u64 {
                if n == 0 {
                    return 0;
                }
                let idx = ((n as f64) * p).min((n - 1) as f64) as usize;
                lats[idx]
            };
            let total_wall_ms: u64 = lats.iter().sum();
            let has_metrics = per_step_has_metrics.get(&step).copied().unwrap_or(false);
            StepStats {
                total_wall_ms,
                latency_p50_ms: percentile(0.50),
                latency_p90_ms: percentile(0.90),
                latency_p99_ms: percentile(0.99),
                latency_max_ms: lats.last().copied().unwrap_or(0),
                count: n,
                failure_count: per_step_failures.get(&step).copied().unwrap_or(0),
                total_scheduling_ms: if has_metrics {
                    Some(per_step_scheduling.get(&step).copied().unwrap_or(0))
                } else {
                    None
                },
                total_network_ms: if has_metrics {
                    Some(per_step_network.get(&step).copied().unwrap_or(0))
                } else {
                    None
                },
                peak_concurrent: per_step_peak_concurrent.get(&step).copied(),
                retried_call_count: per_step_retried.get(&step).copied(),
                step,
            }
        })
        .collect();
    per_step.sort_by(|a, b| b.total_wall_ms.cmp(&a.total_wall_ms));

    let total_llm_wall_ms: u64 = per_step.iter().map(|s| s.total_wall_ms).sum();

    // Parse the day-level log for split/merge/flush counts. Absence is fine.
    let (splits, merges, flushes) = count_day_events(&date_dir.join("day_log.jsonl"));

    Ok(DaySummary {
        date: date.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        session_count,
        splits,
        merges,
        flushes,
        per_step,
        total_llm_wall_ms,
    })
}

/// Scan day_log.jsonl for encounter_split / encounter_merged / continuous_mode_stopped
/// counts. Returns (splits, merges, flushes) with 0s if the file doesn't exist.
fn count_day_events(path: &Path) -> (usize, usize, usize) {
    let mut splits = 0;
    let mut merges = 0;
    let mut flushes = 0;
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (splits, merges, flushes),
    };
    for line in content.lines() {
        let entry: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match entry.get("event").and_then(|v| v.as_str()) {
            Some("encounter_split") => splits += 1,
            Some("encounter_merged") => merges += 1,
            Some("continuous_mode_stopped") => flushes += 1,
            _ => {}
        }
    }
    (splits, merges, flushes)
}

/// Convenience wrapper: compute the summary for today in the default archive
/// root. Called from continuous-mode stop.
pub fn write_today_summary() {
    let now: DateTime<Utc> = Utc::now();
    let date_str = now.format("%Y-%m-%d").to_string();
    // Reuse the existing archive-layout helper: archive_root/{YYYY}/{MM}/{DD}/
    let date_dir = match crate::local_archive::get_archive_dir() {
        Ok(root) => root
            .join(format!("{:04}", now.format("%Y").to_string().parse::<i32>().unwrap_or(0)))
            .join(format!("{:02}", now.format("%m").to_string().parse::<u32>().unwrap_or(0)))
            .join(format!("{:02}", now.format("%d").to_string().parse::<u32>().unwrap_or(0))),
        Err(e) => {
            warn!("Cannot locate archive dir for summary: {}", e);
            return;
        }
    };
    if !date_dir.exists() {
        // Nothing happened today; skip.
        return;
    }
    write_day_summary(&date_dir, &date_str);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_lines(path: &Path, lines: &[&str]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
    }

    #[test]
    fn summary_aggregates_percentiles_across_sessions() {
        let tmp = TempDir::new().unwrap();
        let date_dir = tmp.path();
        // Session A — two detection calls
        write_lines(
            &date_dir.join("session-a").join("pipeline_log.jsonl"),
            &[
                r#"{"step":"encounter_detection","latency_ms":1000,"success":true,"context":{"scheduling_ms":50,"network_ms":950,"concurrent_at_start":0}}"#,
                r#"{"step":"encounter_detection","latency_ms":3000,"success":true,"context":{"scheduling_ms":80,"network_ms":2920,"concurrent_at_start":1}}"#,
            ],
        );
        // Session B — one SOAP + one failed detection
        write_lines(
            &date_dir.join("session-b").join("pipeline_log.jsonl"),
            &[
                r#"{"step":"soap_generation","latency_ms":12000,"success":true,"context":{}}"#,
                r#"{"step":"encounter_detection","latency_ms":40000,"success":false,"context":{"scheduling_ms":200,"network_ms":39800,"concurrent_at_start":2}}"#,
            ],
        );
        // day_log
        write_lines(
            &date_dir.join("day_log.jsonl"),
            &[
                r#"{"event":"encounter_split"}"#,
                r#"{"event":"encounter_split"}"#,
                r#"{"event":"encounter_merged"}"#,
                r#"{"event":"continuous_mode_stopped"}"#,
            ],
        );

        let summary = compute_summary(date_dir, "2026-04-17").unwrap();
        assert_eq!(summary.session_count, 2);
        assert_eq!(summary.splits, 2);
        assert_eq!(summary.merges, 1);
        assert_eq!(summary.flushes, 1);

        let det = summary
            .per_step
            .iter()
            .find(|s| s.step == "encounter_detection")
            .expect("detection step present");
        assert_eq!(det.count, 3);
        assert_eq!(det.failure_count, 1);
        assert_eq!(det.latency_max_ms, 40000);
        assert_eq!(det.total_wall_ms, 44000);
        // All three detection calls had metrics
        assert_eq!(det.total_scheduling_ms, Some(50 + 80 + 200));
        assert_eq!(det.total_network_ms, Some(950 + 2920 + 39800));
        assert_eq!(det.peak_concurrent, Some(2));

        let soap = summary
            .per_step
            .iter()
            .find(|s| s.step == "soap_generation")
            .expect("soap step present");
        // SOAP call had empty context → no metrics
        assert_eq!(soap.total_scheduling_ms, None);
        assert_eq!(soap.total_network_ms, None);
        assert_eq!(soap.peak_concurrent, None);
    }

    #[test]
    fn summary_is_sorted_by_total_wall_ms_desc() {
        let tmp = TempDir::new().unwrap();
        let date_dir = tmp.path();
        write_lines(
            &date_dir.join("s1").join("pipeline_log.jsonl"),
            &[
                r#"{"step":"fast_step","latency_ms":100,"success":true}"#,
                r#"{"step":"slow_step","latency_ms":50000,"success":true}"#,
                r#"{"step":"medium_step","latency_ms":5000,"success":true}"#,
            ],
        );
        let summary = compute_summary(date_dir, "2026-04-17").unwrap();
        assert_eq!(summary.per_step[0].step, "slow_step");
        assert_eq!(summary.per_step[1].step, "medium_step");
        assert_eq!(summary.per_step[2].step, "fast_step");
    }

    #[test]
    fn missing_day_log_is_tolerated() {
        let tmp = TempDir::new().unwrap();
        let date_dir = tmp.path();
        write_lines(
            &date_dir.join("only-session").join("pipeline_log.jsonl"),
            &[r#"{"step":"encounter_detection","latency_ms":500,"success":true}"#],
        );
        let summary = compute_summary(date_dir, "2026-04-17").unwrap();
        assert_eq!(summary.splits, 0);
        assert_eq!(summary.merges, 0);
        assert_eq!(summary.session_count, 1);
    }

    #[test]
    fn empty_directory_returns_zero_sessions() {
        let tmp = TempDir::new().unwrap();
        let summary = compute_summary(tmp.path(), "2026-04-17").unwrap();
        assert_eq!(summary.session_count, 0);
        assert_eq!(summary.per_step.len(), 0);
        assert_eq!(summary.total_llm_wall_ms, 0);
    }

    /// Verification run against a real clinic day. Ignored by default because
    /// it requires the real archive layout; run with:
    /// `cargo test --lib performance_summary::tests::show_real_day -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn show_real_day() {
        let date_dir = Path::new("/Users/backoffice/.transcriptionapp/archive/2026/04/16");
        if !date_dir.exists() {
            println!("skip: test data dir not present");
            return;
        }
        let summary = compute_summary(date_dir, "2026-04-16").unwrap();
        println!("date={} sessions={} splits={} merges={} flushes={} total_llm_wall={}ms",
            summary.date, summary.session_count, summary.splits, summary.merges,
            summary.flushes, summary.total_llm_wall_ms);
        for s in &summary.per_step {
            println!("  {:27} n={:4} p50={:6}ms p90={:6}ms p99={:6}ms max={:6}ms fail={} sched={:?} net={:?} peak_conc={:?}",
                s.step, s.count, s.latency_p50_ms, s.latency_p90_ms, s.latency_p99_ms,
                s.latency_max_ms, s.failure_count, s.total_scheduling_ms, s.total_network_ms,
                s.peak_concurrent);
        }
    }
}
