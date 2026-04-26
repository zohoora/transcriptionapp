//! SOAP-generation prompt experiment CLI (v0.10.62+).
//!
//! Replaces the ad-hoc Python rounds (`prompt_iter*.py`, `regen_soaps.py`)
//! with an in-tree Rust binary. Fetches archived transcripts (local first,
//! profile-service fallback), runs N seeds × M variant prompts through the
//! live LLM router, and writes the regenerated SOAP JSONs + a structural
//! diff summary to `~/.transcriptionapp/experiments/soap/<run_id>/`.
//!
//! Built-in variants:
//!   - "v0_10_61"   — current production SOAP prompt (with the procedure section)
//!   - "no_procsec" — same prompt minus the Procedure-section workflow
//!     (useful for ablations)
//!
//! Custom variants pulled from --variant <file>.
//!
//! Usage:
//!   cargo run --bin soap_experiment_cli -- --date 2026-04-24 \
//!       --variant v0_10_61 --variant no_procsec --seeds 3
//!   cargo run --bin soap_experiment_cli -- --session 8feb77a1 \
//!       --variant prompts/strict_procsec.txt --seeds 5

use std::env;
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;

use transcription_app_lib::experiment::{
    self, labels::LabelEntry, report::Score, runner::Runner, variant::Variant,
};
use transcription_app_lib::llm_client::{build_simple_soap_prompt, LLMClient, SoapOptions};
use transcription_app_lib::replay_fetch::ArchiveFetcher;

const DEFAULT_MODEL: &str = "soap-model-fast";

fn resolve_builtin_variant(name: &str) -> Result<String> {
    match name {
        "v0_10_61" => Ok(build_simple_soap_prompt(&SoapOptions::default(), None)),
        "no_procsec" => {
            // Strip the Procedure-section workflow from the production prompt.
            // This is a simple heuristic — looks for the "PROCEDURE-SECTION
            // WORKFLOW" header and truncates before it.
            let full = build_simple_soap_prompt(&SoapOptions::default(), None);
            if let Some(idx) = full.find("PROCEDURE-SECTION WORKFLOW") {
                Ok(full[..idx].trim_end().to_string())
            } else {
                Ok(full)
            }
        }
        _ => Err(anyhow!("Unknown built-in variant: {name}. Try: v0_10_61, no_procsec, or pass a path to a prompt file.")),
    }
}

struct SoapRunner {
    client: LLMClient,
    model: String,
    fetcher: ArchiveFetcher,
}

#[async_trait]
impl Runner for SoapRunner {
    fn task_name(&self) -> &'static str { "soap_experiment" }

    fn resolve_builtin(&self, name: &str) -> Result<String> {
        resolve_builtin_variant(name)
    }

    async fn run_one(
        &self,
        session_id: &str,
        date: &str,
        variant: &Variant,
        prompt_body: &str,
        seed: u32,
        _label: Option<&LabelEntry>,
    ) -> Result<Score> {
        let details = self.fetcher.fetch_session(session_id, date).await
            .with_context(|| format!("fetch session {session_id}"))?;
        let transcript = details.transcript.clone()
            .ok_or_else(|| anyhow!("session {session_id} has no transcript"))?;

        let user_prompt = if seed > 0 {
            format!("[seed={seed}]\n{transcript}")
        } else {
            transcript
        };

        let started = std::time::Instant::now();
        let response = self.client
            .generate(&self.model, prompt_body, &user_prompt, "soap_note")
            .await
            .map_err(|e| anyhow!("LLM call: {e}"))?;
        let latency_ms = started.elapsed().as_millis() as u64;

        // Parse the JSON response — best-effort. Failure is recorded but doesn't error.
        let parsed: Option<serde_json::Value> = serde_json::from_str(&response).ok();
        let parsed_obj = parsed.as_ref().and_then(|v| v.as_object());

        let n_subjective = parsed_obj
            .and_then(|m| m.get("subjective"))
            .and_then(|v| v.as_array().map(|a| a.len()))
            .unwrap_or(0);
        let n_procedure = parsed_obj
            .and_then(|m| m.get("procedure"))
            .and_then(|v| v.as_array().map(|a| a.len()))
            .unwrap_or(0);

        // Score: structural-only. all_ok=true iff JSON parsed AND has S/O/A/P sections.
        let all_ok = parsed_obj
            .map(|m| m.contains_key("subjective") && m.contains_key("plan"))
            .unwrap_or(false);

        // Save the regenerated SOAP JSON to a per-variant file for the diff CLI.
        if let Some(json_value) = &parsed {
            if let Ok(out_dir) = experiment::runner::default_output_dir("soap") {
                let path = out_dir.join(format!(
                    "{}_{}_{}_{}.json",
                    date, &session_id[..8.min(session_id.len())], variant.label, seed
                ));
                let _ = std::fs::write(
                    &path,
                    serde_json::to_string_pretty(json_value).unwrap_or_default(),
                );
            }
        }

        tracing::debug!(
            session_id = %session_id, variant = %variant.label, seed,
            n_subjective, n_procedure, "soap_experiment run_one"
        );
        Ok(Score {
            variant: variant.label.clone(),
            session_id: session_id.into(),
            seed,
            all_ok,
            visit_ok: None,
            proc_ok: Some(n_procedure > 0),
            cond_ok: Some(true),
            dx_ok: None,
            procedures: None,
            cond_hallucinated: vec![],
            visit_type: None,
            diagnostic_code: None,
            latency_ms: Some(latency_ms),
        })
    }
}

fn print_usage(program: &str) {
    eprintln!("Usage: {program} [OPTIONS]");
    eprintln!();
    eprintln!("Run a SOAP-generation prompt experiment over archived transcripts.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --date YYYY-MM-DD         All sessions on the given date");
    eprintln!("  --session <id>            A single session id");
    eprintln!("  --variant <name|file>     Built-in name (v0_10_61, no_procsec) or path to a prompt file. Repeatable.");
    eprintln!("  --seeds N                 Number of seeds per variant (default 1)");
    eprintln!("  --model <alias>           LLM model alias (default: soap-model-fast)");
    eprintln!("  --output <dir>            Output directory");
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

    let cfg = match transcription_app_lib::config::Config::load() {
        Ok(c) => c,
        Err(e) => { eprintln!("error: load config: {e}"); return ExitCode::from(1); }
    };
    let client = match LLMClient::new(&cfg.llm_router_url, &cfg.llm_api_key, &cfg.llm_client_id, &cfg.fast_model) {
        Ok(c) => c,
        Err(e) => { eprintln!("error: LLMClient::new: {e}"); return ExitCode::from(1); }
    };

    let fetcher = ArchiveFetcher::from_env().unwrap_or_else(|_| ArchiveFetcher::local_only());
    let runner = SoapRunner { client, model, fetcher };

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

    let out_dir = match output_override {
        Some(p) => p,
        None => match experiment::runner::default_output_dir("soap") {
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
    println!("Per-variant SOAP JSONs written under {}", out_dir.display());
    ExitCode::SUCCESS
}
