//! Experiment-runner trait. Each experiment CLI implements `Runner` and the
//! shared [`run_seeded`] driver handles the variant × seed × session cross
//! product.

use std::collections::BTreeMap;
use std::path::PathBuf;
use anyhow::Result;
use async_trait::async_trait;

use super::report::Score;
use super::variant::Variant;
use super::labels::LabelEntry;

/// What an experiment knows how to do, generically.
///
/// Each CLI implements this trait once and gets multi-seed × multi-variant
/// orchestration for free.
#[async_trait]
pub trait Runner: Send + Sync {
    /// Display name (e.g., "soap_experiment", "billing_experiment").
    fn task_name(&self) -> &'static str;

    /// Resolve a built-in variant name to its prompt body. Called once per
    /// `Variant` of [`super::variant::VariantSource::Builtin`] type. Return
    /// `Err` for unknown names.
    fn resolve_builtin(&self, name: &str) -> Result<String>;

    /// Run one (session × variant × seed) combination and produce a [`Score`].
    /// `prompt_body` is the resolved variant prompt; `label` is the optional
    /// ground truth (present when the labels corpus has an entry for this
    /// session). Implementations should call the LLM router internally.
    async fn run_one(
        &self,
        session_id: &str,
        date: &str,
        variant: &Variant,
        prompt_body: &str,
        seed: u32,
        label: Option<&LabelEntry>,
    ) -> Result<Score>;
}

/// Run every (variant × seed) combo over `sessions`. Returns one [`Score`]
/// per combination. Variants whose `Builtin` name doesn't resolve are skipped
/// with a warning. Sessions without labels still run; `label` is `None`.
///
/// Up to `RUN_SEEDED_CONCURRENCY` LLM calls run in parallel via
/// `futures::stream::buffer_unordered`. Concurrency is capped to keep the
/// shared LLM router responsive — full clinic-week experiments (~700
/// combinations) finish in ~1/8 the wall-clock time vs serial.
pub async fn run_seeded<R: Runner>(
    runner: &R,
    sessions: &[(String, String)], // (session_id, date)
    variants: &[Variant],
    seeds: u32,
) -> Vec<Score> {
    use futures_util::stream::{self, StreamExt};

    // Pre-resolve variant bodies once.
    let mut bodies: BTreeMap<String, String> = BTreeMap::new();
    for v in variants {
        match v.body(|name| runner.resolve_builtin(name)) {
            Ok(body) => { bodies.insert(v.label.clone(), body); }
            Err(e) => tracing::warn!(variant = %v.label, error = %e, "Skipping variant — resolver failed"),
        }
    }

    // Build the work units up front, then drive them through buffer_unordered.
    // Each unit is `(sid, date, variant_idx, seed)` — borrowed views into the
    // input slices and the bodies map are passed via the returned future.
    let mut units: Vec<(usize, usize, u32)> = Vec::with_capacity(
        sessions.len() * variants.len() * seeds as usize,
    );
    for (s_idx, _) in sessions.iter().enumerate() {
        for (v_idx, v) in variants.iter().enumerate() {
            if !bodies.contains_key(&v.label) { continue }
            for seed in 0..seeds {
                units.push((s_idx, v_idx, seed));
            }
        }
    }

    stream::iter(units)
        .map(|(s_idx, v_idx, seed)| {
            let (sid, date) = &sessions[s_idx];
            let v = &variants[v_idx];
            let body = bodies.get(&v.label).expect("checked above");
            async move {
                let label = super::labels::load_label_for_session(sid, date);
                runner.run_one(sid, date, v, body, seed, label.as_ref()).await
                    .map_err(|e| (sid.clone(), v.label.clone(), seed, e))
            }
        })
        .buffer_unordered(RUN_SEEDED_CONCURRENCY)
        .filter_map(|r| async move {
            match r {
                Ok(score) => Some(score),
                Err((sid, variant, seed, e)) => {
                    tracing::warn!(session_id = %sid, variant = %variant, seed, error = %e, "run_one failed");
                    None
                }
            }
        })
        .collect()
        .await
}

/// Max concurrent LLM calls in [`run_seeded`]. Set conservatively so a
/// shared LLM router stays responsive for production traffic running
/// alongside an experiment. Override at the call site by re-implementing
/// `run_seeded` if you need higher throughput.
pub const RUN_SEEDED_CONCURRENCY: usize = 8;

/// Default output directory for experiment runs:
/// `~/.transcriptionapp/experiments/<task>/<run_id>/`.
/// `run_id` is a UTC timestamp at second granularity.
pub fn default_output_dir(task: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no HOME"))?;
    let run_id = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let dir = home
        .join(".transcriptionapp")
        .join("experiments")
        .join(task)
        .join(&run_id);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::variant::Variant;

    struct EchoRunner;

    #[async_trait]
    impl Runner for EchoRunner {
        fn task_name(&self) -> &'static str { "echo" }
        fn resolve_builtin(&self, name: &str) -> Result<String> {
            Ok(format!("body:{name}"))
        }
        async fn run_one(
            &self,
            session_id: &str,
            _date: &str,
            variant: &Variant,
            _prompt_body: &str,
            seed: u32,
            _label: Option<&LabelEntry>,
        ) -> Result<Score> {
            Ok(Score {
                variant: variant.label.clone(),
                session_id: session_id.to_string(),
                seed,
                all_ok: true,
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn test_run_seeded_produces_score_per_combo() {
        let r = EchoRunner;
        let sessions = vec![("s1".into(), "2026-04-01".into()), ("s2".into(), "2026-04-01".into())];
        let variants = vec![Variant::builtin("a"), Variant::builtin("b")];
        let scores = run_seeded(&r, &sessions, &variants, 3).await;
        assert_eq!(scores.len(), 2 * 2 * 3);
        assert!(scores.iter().all(|s| s.all_ok));
    }

    #[tokio::test]
    async fn test_run_seeded_skips_unresolvable_variant() {
        struct R2;
        #[async_trait]
        impl Runner for R2 {
            fn task_name(&self) -> &'static str { "r2" }
            fn resolve_builtin(&self, name: &str) -> Result<String> {
                if name == "good" { Ok("ok".into()) } else { Err(anyhow::anyhow!("bad name")) }
            }
            async fn run_one(
                &self,
                session_id: &str,
                _date: &str,
                variant: &Variant,
                _prompt_body: &str,
                seed: u32,
                _label: Option<&LabelEntry>,
            ) -> Result<Score> {
                Ok(Score {
                    variant: variant.label.clone(),
                    session_id: session_id.into(),
                    seed,
                    all_ok: true,
                    ..Default::default()
                })
            }
        }
        let r = R2;
        let sessions = vec![("s1".into(), "2026-04-01".into())];
        let variants = vec![Variant::builtin("good"), Variant::builtin("bad")];
        let scores = run_seeded(&r, &sessions, &variants, 1).await;
        // Only "good" should produce scores
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].variant, "good");
    }
}
