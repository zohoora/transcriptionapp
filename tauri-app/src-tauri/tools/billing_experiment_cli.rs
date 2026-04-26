//! Billing-extraction prompt experiment CLI (v0.10.62+).
//!
//! Replaces the ad-hoc Python rounds (`tests/fixtures/billing_sim_2026_04_24/sim/round*.py`)
//! with an in-tree Rust binary. Loads archived sessions (local first, profile-service
//! fallback), runs N seeds × M variant prompts through the live LLM router, scores
//! against `tests/fixtures/labels/*.json`, and writes a per-run results JSON +
//! markdown summary under `~/.transcriptionapp/experiments/billing/<run_id>/`.
//!
//! Built-in variants:
//!   - "baseline" — current production prompt at HEAD (the default)
//!   - "visit_dx" — baseline + visit-type calibration guide + dx chain-of-thought
//!     (the v0.10.61 winner identified during the Apr 24 simulation)
//!   - "strict"   — visit_dx + extra strict-condition rider (re-emphasizes
//!     the diabetic_assessment / chf_management / smoking_cessation rules)
//!
//! Custom variants pulled from --variant <file>.
//!
//! Usage:
//!   cargo run --bin billing_experiment_cli -- --date 2026-04-24 \
//!       --variant baseline --variant visit_dx --variant strict --seeds 3
//!   cargo run --bin billing_experiment_cli -- --session deb5f823 \
//!       --variant prompts/my_test.txt --seeds 5

use std::env;
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;

use transcription_app_lib::billing::clinical_features::{
    build_billing_extraction_prompt, enum_to_snake_key, parse_billing_extraction,
};
use transcription_app_lib::billing::rule_engine::condition_keyword_guard;
use transcription_app_lib::experiment::{
    self, labels::LabelEntry, report::Score, runner::Runner, variant::Variant,
};
use transcription_app_lib::llm_client::LLMClient;
use transcription_app_lib::replay_fetch::ArchiveFetcher;

const DEFAULT_MODEL: &str = "fast-model";

// ── Built-in variants ──────────────────────────────────────────────────────

const VISIT_GUIDE_RIDER: &str = include_str!("billing_experiment_riders/visit_guide.txt");
const DX_COT_RIDER: &str = include_str!("billing_experiment_riders/dx_cot.txt");
const STRICT_COND_RIDER: &str = include_str!("billing_experiment_riders/strict_cond.txt");

fn baseline_prompt(soap_content: &str, transcript: &str, ctx_hints: &str) -> (String, String) {
    build_billing_extraction_prompt(soap_content, transcript, ctx_hints, None)
}

fn resolve_builtin_variant(name: &str) -> Result<String> {
    // The "body" we return here is just the rider text appended to the
    // baseline prompt at call time. For pure baseline, return empty rider.
    match name {
        "baseline" => Ok(String::new()),
        "visit_dx" => Ok(format!("{}{}", VISIT_GUIDE_RIDER, DX_COT_RIDER)),
        "strict" => Ok(format!("{}{}{}", VISIT_GUIDE_RIDER, DX_COT_RIDER, STRICT_COND_RIDER)),
        _ => Err(anyhow!("Unknown built-in variant: {name}. Try: baseline, visit_dx, strict, or pass a path to a prompt file.")),
    }
}

// ── Runner impl ────────────────────────────────────────────────────────────

struct BillingRunner {
    client: LLMClient,
    model: String,
    fetcher: ArchiveFetcher,
}

#[async_trait]
impl Runner for BillingRunner {
    fn task_name(&self) -> &'static str { "billing_experiment" }

    fn resolve_builtin(&self, name: &str) -> Result<String> {
        resolve_builtin_variant(name)
    }

    async fn run_one(
        &self,
        session_id: &str,
        date: &str,
        variant: &Variant,
        prompt_body_rider: &str,
        seed: u32,
        label: Option<&LabelEntry>,
    ) -> Result<Score> {
        let details = self.fetcher.fetch_session(session_id, date).await
            .with_context(|| format!("fetch session {session_id}"))?;
        let soap = details.soap_note.clone().unwrap_or_default();
        let transcript = details.transcript.clone().unwrap_or_default();

        // Build base prompt + append the variant rider.
        let (mut system_prompt, user_prompt) = baseline_prompt(&soap, &transcript, "");
        if !prompt_body_rider.is_empty() {
            system_prompt.push('\n');
            system_prompt.push_str(prompt_body_rider);
        }

        // Force seed-influenced jitter via temperature + a `seed` tag in the
        // user prompt header. The LLM router doesn't expose a seed param, so
        // we force variance by appending the seed integer to the prompt
        // (negligible on accuracy, sufficient for variance).
        let user_prompt = if seed > 0 {
            format!("[seed={seed}]\n{user_prompt}")
        } else {
            user_prompt
        };

        let started = std::time::Instant::now();
        let response = self.client
            .generate(&self.model, &system_prompt, &user_prompt, "billing_extraction")
            .await
            .map_err(|e| anyhow!("LLM call: {e}"))?;
        let latency_ms = started.elapsed().as_millis() as u64;

        // Parse the response (uses production parser — last-JSON + self-negation guard).
        let mut features = match parse_billing_extraction(&response) {
            Ok(f) => f,
            Err(e) => {
                // Parse failure is itself a finding — record it.
                return Ok(Score {
                    variant: variant.label.clone(),
                    session_id: session_id.into(),
                    seed,
                    all_ok: false,
                    visit_ok: Some(false),
                    proc_ok: Some(false),
                    cond_ok: Some(true),
                    dx_ok: Some(false),
                    procedures: None,
                    cond_hallucinated: vec![format!("PARSE_ERROR: {e}")],
                    visit_type: None,
                    diagnostic_code: None,
                    latency_ms: Some(latency_ms),
                });
            }
        };

        // Apply the SOAP-text keyword guard (matches production).
        let (kept, _dropped) = condition_keyword_guard(&features.conditions, &soap);
        features.conditions = kept;

        let visit_type_str = enum_to_snake_key(&features.visit_type);
        let dx = features.suggested_diagnostic_code.clone().unwrap_or_default();
        let procs: Vec<String> = features.procedures.iter().filter_map(enum_to_snake_key).collect();
        let conds: Vec<String> = features.conditions.iter().filter_map(enum_to_snake_key).collect();

        // Compute per-dimension flags first, then derive all_ok at the end.
        // visit_type / procedures aren't yet scored in v0.10.62 — the harness
        // currently only flags rule-engine-stage hallucinations + dx mismatch.
        // Procedure-level scoring requires running map_features_to_billing in
        // a follow-up.
        let visit_ok = Some(true);
        let proc_ok = Some(true);

        let cond_hallucinated = match label {
            Some(label) if notes_flag_hallucinations(label) => conds
                .iter()
                .filter(|c| matches!(c.as_str(), "diabetic_assessment" | "chf_management" | "smoking_cessation"))
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        let cond_ok = Some(cond_hallucinated.is_empty());

        let dx_ok = label
            .and_then(|l| l.labels.diagnostic_code_expected.as_deref())
            .map(|expected| dx == expected);

        let labelled = label.is_some();
        let all_ok = labelled
            && visit_ok.unwrap_or(true)
            && proc_ok.unwrap_or(true)
            && cond_ok.unwrap_or(true)
            && dx_ok.unwrap_or(true);

        Ok(Score {
            variant: variant.label.clone(),
            session_id: session_id.into(),
            seed,
            all_ok,
            visit_ok,
            proc_ok,
            cond_ok,
            dx_ok,
            procedures: Some(procs),
            cond_hallucinated,
            visit_type: visit_type_str,
            diagnostic_code: Some(dx),
            latency_ms: Some(latency_ms),
        })
    }
}

/// Flag a label as one whose `notes` field opts it into K-code-hallucination
/// scoring. Existing v0.10.59-corpus labels use free-text notes, so we look
/// for the verbatim phrasing the clinician + scrubbed-corpus tooling produce.
fn notes_flag_hallucinations(label: &LabelEntry) -> bool {
    label
        .labels
        .notes
        .as_deref()
        .map(|n| {
            let l = n.to_lowercase();
            l.contains("hallucinated") || l.contains("forbidden") || l.contains("not present")
        })
        .unwrap_or(false)
}

// ── CLI plumbing ───────────────────────────────────────────────────────────

fn print_usage(program: &str) {
    eprintln!("Usage: {program} [OPTIONS]");
    eprintln!();
    eprintln!("Run a billing-extraction prompt experiment over archived sessions.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --date YYYY-MM-DD         All sessions on the given date");
    eprintln!("  --session <short_id>      A single session id (8-char prefix or full UUID)");
    eprintln!("  --variant <name|file>     Built-in name (baseline, visit_dx, strict) or path to a prompt file. Repeatable.");
    eprintln!("  --seeds N                 Number of seeds per variant (default 1)");
    eprintln!("  --model <alias>           LLM model alias (default: fast-model)");
    eprintln!("  --output <dir>            Output directory (default: ~/.transcriptionapp/experiments/billing/<run_id>/)");
    eprintln!("  --help                    This message");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];
    if args.contains(&"--help".to_string()) || args.len() < 2 {
        print_usage(program);
        return if args.len() < 2 { ExitCode::from(1) } else { ExitCode::SUCCESS };
    }

    let mut date: Option<String> = None;
    let mut session: Option<String> = None;
    let mut variants: Vec<Variant> = Vec::new();
    let mut seeds: u32 = 1;
    let mut model = DEFAULT_MODEL.to_string();
    let mut output_override: Option<std::path::PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--date" => { i += 1; date = Some(args[i].clone()); }
            "--session" => { i += 1; session = Some(args[i].clone()); }
            "--variant" => { i += 1; variants.push(experiment::variant::parse_variant_arg(&args[i])); }
            "--seeds" => { i += 1; seeds = args[i].parse().unwrap_or(1); }
            "--model" => { i += 1; model = args[i].clone(); }
            "--output" => { i += 1; output_override = Some(args[i].clone().into()); }
            "--help" => { print_usage(program); return ExitCode::SUCCESS; }
            other if other.starts_with('-') => {
                eprintln!("Unknown option: {other}");
                return ExitCode::from(1);
            }
            _ => {}
        }
        i += 1;
    }

    if variants.is_empty() {
        eprintln!("error: at least one --variant is required");
        return ExitCode::from(1);
    }
    if date.is_none() && session.is_none() {
        eprintln!("error: --date or --session is required");
        return ExitCode::from(1);
    }

    // Build LLM client from local config.
    let cfg = match transcription_app_lib::config::Config::load() {
        Ok(c) => c,
        Err(e) => { eprintln!("error: load config: {e}"); return ExitCode::from(1); }
    };
    let client = match LLMClient::new(&cfg.llm_router_url, &cfg.llm_api_key, &cfg.llm_client_id, &cfg.fast_model) {
        Ok(c) => c,
        Err(e) => { eprintln!("error: LLMClient::new: {e}"); return ExitCode::from(1); }
    };

    let fetcher = ArchiveFetcher::from_env().unwrap_or_else(|_| ArchiveFetcher::local_only());
    let runner = BillingRunner { client, model, fetcher };

    // Resolve target sessions.
    let mut targets: Vec<(String, String)> = Vec::new();
    if let Some(date_str) = &date {
        let summaries = match runner.fetcher.list_sessions_for_date(date_str).await {
            Ok(s) => s,
            Err(e) => { eprintln!("error: list sessions: {e}"); return ExitCode::from(1); }
        };
        for s in summaries {
            targets.push((s.session_id, date_str.clone()));
        }
    }
    if let Some(sid) = &session {
        // For --session, default the date to today; full UUID required for fetch.
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        targets.push((sid.clone(), date.clone().unwrap_or(today)));
    }

    if targets.is_empty() {
        eprintln!("warn: no sessions matched. Nothing to run.");
        return ExitCode::SUCCESS;
    }

    println!("Running {} variant(s) × {} seed(s) × {} session(s) = {} LLM calls",
             variants.len(), seeds, targets.len(), variants.len() * seeds as usize * targets.len());

    let scores = experiment::runner::run_seeded(&runner, &targets, &variants, seeds).await;
    let aggregates = experiment::report::aggregate(&scores);

    // Write outputs.
    let out_dir = match output_override {
        Some(p) => p,
        None => match experiment::runner::default_output_dir("billing") {
            Ok(p) => p,
            Err(e) => { eprintln!("error: output dir: {e}"); return ExitCode::from(1); }
        }
    };
    let json_path = out_dir.join("results.json");
    if let Err(e) = experiment::report::write_results_json(&json_path, &scores, &aggregates) {
        eprintln!("error: write results: {e}");
        return ExitCode::from(1);
    }
    let md = experiment::report::aggregate_markdown(&aggregates);
    let md_path = out_dir.join("summary.md");
    let _ = std::fs::write(&md_path, &md);

    let perf_path = out_dir.join("performance_summary.json");
    if let Err(e) = experiment::report::write_performance_summary_json(&perf_path, &scores) {
        eprintln!("warn: write performance_summary.json: {e}");
    }

    println!("\n{md}");
    println!("Results written to {}", out_dir.display());
    ExitCode::SUCCESS
}
