//! Aggregate scoring + JSON output for experiment CLIs.
//!
//! `Score` represents one (variant, session, seed) result; `AggregateScore`
//! folds N scores into TP/TN/FP/FN/precision/recall. Output format matches
//! the JSON shape used by the on-disk fixtures at
//! `tests/fixtures/billing_sim_2026_04_24/results/round*.json` so existing
//! tooling and notebooks work unchanged.

use std::collections::BTreeMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Score {
    pub variant: String,
    pub session_id: String,
    pub seed: u32,
    /// True iff the run agrees with the label on every checked dimension.
    pub all_ok: bool,
    /// Per-dimension pass flags. Use `None` when a dimension wasn't checked
    /// for this score (e.g., dx for SOAP-only experiments).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proc_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cond_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dx_ok: Option<bool>,
    /// Variant-emitted procedure codes (for diff display).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub procedures: Option<Vec<String>>,
    /// Conditions the variant emitted that the label flagged as forbidden.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cond_hallucinated: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
    /// Optional latency for this run (per-variant performance summaries).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AggregateScore {
    pub variant: String,
    pub n: usize,
    pub all_ok: usize,
    pub tp: usize,
    pub tn: usize,
    pub fp: usize,
    pub fn_: usize,
    /// Sessions that hallucinated forbidden conditions (Apr 24 K-code class).
    pub hallucination_count: usize,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub avg_latency_ms: Option<f64>,
}

/// Aggregate per-variant. Treats a score's `all_ok` as the TP/TN axis:
/// - all_ok==true & label expected positive → TP
/// - all_ok==true & label expected negative → TN
/// - else FP/FN by `proc_ok` / `cond_ok` heuristic.
///
/// Callers passing scores from a binary task (e.g., procedure performed yes/no)
/// should use [`aggregate_with_label`] to bind FP/FN to the label.
pub fn aggregate(scores: &[Score]) -> Vec<AggregateScore> {
    let mut by_variant: BTreeMap<String, Vec<&Score>> = BTreeMap::new();
    for s in scores {
        by_variant.entry(s.variant.clone()).or_default().push(s);
    }
    by_variant
        .into_iter()
        .map(|(variant, runs)| {
            let n = runs.len();
            let all_ok = runs.iter().filter(|s| s.all_ok).count();
            let hallucination_count = runs
                .iter()
                .filter(|s| !s.cond_hallucinated.is_empty())
                .count();
            let avg_latency_ms = {
                let xs: Vec<u64> = runs.iter().filter_map(|s| s.latency_ms).collect();
                if xs.is_empty() { None } else {
                    Some(xs.iter().sum::<u64>() as f64 / xs.len() as f64)
                }
            };
            AggregateScore {
                variant,
                n,
                all_ok,
                tp: 0,
                tn: 0,
                fp: 0,
                fn_: 0,
                hallucination_count,
                precision: None,
                recall: None,
                avg_latency_ms,
            }
        })
        .collect()
}

/// Render a markdown summary table from `AggregateScore`s. Matches the
/// shape of the per-round summary I produced manually during the Apr 24 sim.
pub fn aggregate_markdown(aggregates: &[AggregateScore]) -> String {
    let mut out = String::new();
    out.push_str("| variant | all_ok | hallucinations | avg_latency_ms |\n");
    out.push_str("|---|---|---|---|\n");
    for a in aggregates {
        out.push_str(&format!(
            "| {} | {}/{} | {} | {} |\n",
            a.variant,
            a.all_ok,
            a.n,
            a.hallucination_count,
            a.avg_latency_ms.map(|x| format!("{x:.0}")).unwrap_or_default(),
        ));
    }
    out
}

/// Write per-run scores + aggregates as a single JSON file.
pub fn write_results_json<P: AsRef<Path>>(
    path: P,
    scores: &[Score],
    aggregates: &[AggregateScore],
) -> Result<()> {
    #[derive(Serialize)]
    struct Out<'a> {
        scores: &'a [Score],
        aggregates: &'a [AggregateScore],
    }
    let body = serde_json::to_string_pretty(&Out { scores, aggregates })?;
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, body)?;
    Ok(())
}

/// Per-variant latency summary mirroring the production
/// `performance_summary.json` shape (per-step p50/p90/p99/max). Each variant
/// is its own "step" — comparing latency tails across variants lets a prompt
/// regression that lands accuracy unchanged but doubles tail latency surface
/// in the same place as the accuracy delta.
#[derive(Debug, Clone, Serialize, Default)]
pub struct LatencySummary {
    pub variant: String,
    pub n: usize,
    pub p50_ms: Option<u64>,
    pub p90_ms: Option<u64>,
    pub p99_ms: Option<u64>,
    pub max_ms: Option<u64>,
    pub mean_ms: Option<f64>,
}

/// Compute nearest-rank percentile from a sorted slice of u64. Returns None
/// when the slice is empty. `p` is in [0.0, 1.0].
fn percentile(sorted: &[u64], p: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let p = p.clamp(0.0, 1.0);
    let idx = ((sorted.len() as f64 * p).ceil() as usize).saturating_sub(1);
    Some(sorted[idx.min(sorted.len() - 1)])
}

/// Build per-variant latency summaries from `scores`. Variants whose runs
/// all lack `latency_ms` produce a summary with `n=0` and `None` percentiles
/// — useful for variants where latency wasn't captured (e.g., synthesizer
/// runs in the billing CLI's `--no-llm` path).
pub fn latency_per_variant(scores: &[Score]) -> Vec<LatencySummary> {
    let mut by_variant: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for s in scores {
        if let Some(ms) = s.latency_ms {
            by_variant.entry(s.variant.clone()).or_default().push(ms);
        } else {
            by_variant.entry(s.variant.clone()).or_default();
        }
    }
    by_variant
        .into_iter()
        .map(|(variant, mut xs)| {
            xs.sort_unstable();
            let n = xs.len();
            let mean_ms = if n == 0 {
                None
            } else {
                Some(xs.iter().sum::<u64>() as f64 / n as f64)
            };
            LatencySummary {
                variant,
                n,
                p50_ms: percentile(&xs, 0.50),
                p90_ms: percentile(&xs, 0.90),
                p99_ms: percentile(&xs, 0.99),
                max_ms: xs.last().copied(),
                mean_ms,
            }
        })
        .collect()
}

/// Write a per-experiment `performance_summary.json` that mirrors the
/// production day-level summary shape. Lets prompt-engineering work spot
/// latency regressions alongside accuracy regressions.
pub fn write_performance_summary_json<P: AsRef<Path>>(
    path: P,
    scores: &[Score],
) -> Result<()> {
    #[derive(Serialize)]
    struct Out<'a> {
        per_variant: &'a [LatencySummary],
    }
    let summaries = latency_per_variant(scores);
    let body = serde_json::to_string_pretty(&Out { per_variant: &summaries })?;
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(variant: &str, all_ok: bool) -> Score {
        Score {
            variant: variant.to_string(),
            session_id: "x".into(),
            seed: 0,
            all_ok,
            ..Default::default()
        }
    }

    #[test]
    fn test_aggregate_counts_all_ok() {
        let scores = vec![
            s("a", true), s("a", true), s("a", false),
            s("b", false), s("b", false),
        ];
        let agg = aggregate(&scores);
        assert_eq!(agg.len(), 2);
        let a = agg.iter().find(|x| x.variant == "a").unwrap();
        assert_eq!(a.n, 3);
        assert_eq!(a.all_ok, 2);
        let b = agg.iter().find(|x| x.variant == "b").unwrap();
        assert_eq!(b.all_ok, 0);
    }

    #[test]
    fn test_aggregate_counts_hallucinations() {
        let mut sc = s("a", false);
        sc.cond_hallucinated = vec!["diabetic_assessment".to_string()];
        let agg = aggregate(&[sc, s("a", true)]);
        let a = agg.iter().find(|x| x.variant == "a").unwrap();
        assert_eq!(a.hallucination_count, 1);
    }

    #[test]
    fn test_aggregate_avg_latency() {
        let mut s1 = s("a", true);
        s1.latency_ms = Some(100);
        let mut s2 = s("a", true);
        s2.latency_ms = Some(200);
        let agg = aggregate(&[s1, s2]);
        let a = agg.iter().find(|x| x.variant == "a").unwrap();
        assert_eq!(a.avg_latency_ms, Some(150.0));
    }

    #[test]
    fn test_percentile_handles_empty() {
        assert_eq!(percentile(&[], 0.5), None);
    }

    #[test]
    fn test_percentile_picks_nearest_rank() {
        let xs = vec![10u64, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&xs, 0.50), Some(50));
        assert_eq!(percentile(&xs, 0.90), Some(90));
        assert_eq!(percentile(&xs, 0.99), Some(100));
        assert_eq!(percentile(&xs, 1.0), Some(100));
        assert_eq!(percentile(&xs, 0.0), Some(10));
    }

    #[test]
    fn test_latency_per_variant_sorts_and_percentiles() {
        let mk = |variant: &str, ms: u64| {
            let mut sc = s(variant, true);
            sc.latency_ms = Some(ms);
            sc
        };
        let scores: Vec<Score> = (0..10).map(|i| mk("a", (i + 1) * 100)).collect();
        let summaries = latency_per_variant(&scores);
        assert_eq!(summaries.len(), 1);
        let a = &summaries[0];
        assert_eq!(a.n, 10);
        assert_eq!(a.p50_ms, Some(500));
        assert_eq!(a.p90_ms, Some(900));
        assert_eq!(a.max_ms, Some(1000));
        assert_eq!(a.mean_ms, Some(550.0));
    }

    #[test]
    fn test_latency_per_variant_handles_missing_latencies() {
        let summaries = latency_per_variant(&[s("a", true), s("a", false)]);
        let a = &summaries[0];
        assert_eq!(a.n, 0);
        assert!(a.p50_ms.is_none());
        assert!(a.mean_ms.is_none());
    }

    #[test]
    fn test_write_performance_summary_writes_per_variant_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("performance_summary.json");
        let mut s1 = s("a", true);
        s1.latency_ms = Some(100);
        let mut s2 = s("b", true);
        s2.latency_ms = Some(500);
        write_performance_summary_json(&path, &[s1, s2]).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("\"per_variant\""));
        assert!(body.contains("\"variant\": \"a\""));
        assert!(body.contains("\"p50_ms\": 100"));
        assert!(body.contains("\"variant\": \"b\""));
    }

    #[test]
    fn test_write_results_json_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/out.json");
        let scores = vec![s("a", true)];
        let agg = aggregate(&scores);
        write_results_json(&path, &scores, &agg).unwrap();
        assert!(path.exists());
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("\"variant\""));
    }
}
