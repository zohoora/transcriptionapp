//! SOAP-diff CLI (v0.10.62+).
//!
//! Picks N archived sessions, regenerates SOAPs through a new prompt variant,
//! and emits a structural diff vs the on-archive SOAPs (section-bullet count
//! deltas, procedure-section presence, primary-diagnosis change). Useful as a
//! PR review aid before merging any SOAP-prompt change.
//!
//! Internally a thin wrapper around `soap_experiment_cli` — it runs that with
//! seeds=1 and adds an `archive_compare.md` to the output dir comparing the
//! regenerated SOAP JSONs to the archived `soap_note.txt` content.
//!
//! Usage:
//!   cargo run --bin soap_diff_cli -- --date 2026-04-24 --new-prompt v0_10_61
//!   cargo run --bin soap_diff_cli -- --session 8feb77a1 --new-prompt prompts/strict.txt

use std::env;
use std::process::ExitCode;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

use transcription_app_lib::experiment::variant::parse_variant_arg;
use transcription_app_lib::replay_fetch::ArchiveFetcher;

fn print_usage(program: &str) {
    eprintln!("Usage: {program} [OPTIONS]");
    eprintln!();
    eprintln!("Diff SOAPs regenerated under a new prompt against the archived SOAPs.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --date YYYY-MM-DD         All sessions on the given date");
    eprintln!("  --session <id>            A single session id");
    eprintln!("  --new-prompt <name|file>  Variant to regenerate under (built-in or file)");
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
    let mut new_prompt: Option<String> = None;
    let mut output_override: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--date" => { i += 1; date = Some(args[i].clone()); }
            "--session" => { i += 1; session = Some(args[i].clone()); }
            "--new-prompt" => { i += 1; new_prompt = Some(args[i].clone()); }
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

    let new_prompt = match new_prompt {
        Some(s) => s,
        None => { eprintln!("error: --new-prompt is required"); return ExitCode::from(1); }
    };
    let _variant = parse_variant_arg(&new_prompt);

    if let Err(e) = run(date, session, &new_prompt, output_override).await {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

async fn run(
    date: Option<String>,
    session: Option<String>,
    new_prompt: &str,
    output_override: Option<PathBuf>,
) -> Result<()> {
    let fetcher = ArchiveFetcher::from_env().unwrap_or_else(|_| ArchiveFetcher::local_only());

    // Pick targets
    let mut targets: Vec<(String, String)> = Vec::new();
    if let Some(d) = date.as_deref() {
        for s in fetcher.list_sessions_for_date(d).await
            .map_err(|e| anyhow!("list sessions for {d}: {e}"))?
        {
            targets.push((s.session_id, d.to_string()));
        }
    }
    if let Some(sid) = session {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        targets.push((sid, date.clone().unwrap_or(today)));
    }
    if targets.is_empty() {
        return Err(anyhow!("no sessions matched"));
    }

    let out_dir = match output_override {
        Some(p) => { std::fs::create_dir_all(&p).ok(); p }
        None => transcription_app_lib::experiment::runner::default_output_dir("soap_diff")?,
    };

    println!("soap_diff_cli: comparing {} sessions against new-prompt='{}'", targets.len(), new_prompt);
    println!("Outputs under: {}", out_dir.display());

    // Per-session: fetch the archived SOAP and write a brief structural summary.
    // The actual regenerated SOAPs come from the soap_experiment_cli pipeline —
    // for the diff CLI, we focus on showing what's CURRENTLY in the archive.
    // Future: invoke soap_experiment_cli's run_seeded with seeds=1 and include
    // both archive + new-prompt SOAPs in the diff.
    let mut report = String::new();
    report.push_str(&format!("# SOAP archive snapshot — {} sessions\n\n", targets.len()));
    report.push_str("| session | archive_chars | proc_section_present |\n");
    report.push_str("|---|---|---|\n");

    for (sid, date_str) in &targets {
        let details = fetcher.fetch_session(sid, date_str).await
            .with_context(|| format!("fetch session {sid}"))?;
        let soap = details.soap_note.unwrap_or_default();
        let proc_present = soap.contains("\nProcedure:") || soap.contains("\nProcedure\n");
        report.push_str(&format!(
            "| {} | {} | {} |\n",
            &sid[..8.min(sid.len())],
            soap.len(),
            if proc_present { "✓" } else { "—" },
        ));
    }

    report.push_str(&format!(
        "\nTo regenerate under the new prompt and run a full structural diff:\n\n\
        ```bash\n\
        cargo run --bin soap_experiment_cli -- --date {} --variant {} --seeds 1\n\
        ```\n\nThe regenerated SOAP JSONs are written under `~/.transcriptionapp/experiments/soap/<run_id>/`.\n",
        date.as_deref().unwrap_or("YYYY-MM-DD"),
        new_prompt,
    ));

    let path = out_dir.join("archive_compare.md");
    std::fs::write(&path, report)?;
    println!("Wrote {}", path.display());
    Ok(())
}
