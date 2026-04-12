//! Replay Bundle Backfill Tool
//!
//! Walks every `replay_bundle.json` in the archive, reconstructs the
//! `sensor_continuous_present` field for each detection check from the
//! corresponding day's mmWave CSV and `day_log.jsonl`, and rewrites the
//! bundle with `schema_version: 2`.
//!
//! See `detection_replay_cli` for how this field affects replay outcomes.
//! Briefly: production raises the LLM-only split threshold to 0.99 when
//! the sensor has remained continuously Present since the last split.
//! Historical bundles (schema v1) stored no evidence of that flag, so
//! `detection_replay_cli` defaulted it to `false` and mis-replayed any
//! check where production actually blocked a split via the 0.99 gate.
//!
//! Usage:
//!   cargo run --bin replay_bundle_backfill -- --dry-run
//!   cargo run --bin replay_bundle_backfill
//!   cargo run --bin replay_bundle_backfill -- --archive ~/.transcriptionapp/archive/2026/03/20
//!
//! Idempotent: bundles already at `schema_version >= 2` are skipped.
//!
//! Historical caveat: bundles written before the merge-back finalization
//! fix (~2026-04, see `ReplayBundleBuilder::build_merged_and_reset`) may
//! contain aggregated state from multiple encounters when merge-back occurred.
//! Those bundles are left as-is — the leak isn't recoverable from bundle
//! data alone, and a cleanup pass that mutates historical data is net-
//! negative risk. Newer bundles use sibling-file separation via
//! `replay_bundle.merged_{short_id}.json`.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Value};

/// Date the `sensor_continuous_present` gate was introduced in production
/// (commit `a214c48` on 2026-04-01 at 21:57 UTC, shipped in v0.9.3 at
/// 22:07 UTC). Bundles captured before this date never had the gate, so
/// they must be backfilled with `false` regardless of what the sensor data
/// shows — otherwise we'd retroactively apply post-April-1 logic to older
/// historical data and manufacture spurious `BelowThreshold` results.
///
/// We use 2026-04-02 00:00 UTC (day after the commit) as the cutoff rather
/// than the commit time itself because:
///   1. The commit → build → tag → auto-update → app restart chain takes
///      time, and room machines only pick up new versions on restart
///   2. All 5 mismatched April 1 bundles were captured BEFORE the commit
///      time, confirming they ran pre-feature code
///   3. A day-level cutoff is simpler to reason about than a sub-day one
///
/// Change this only if you move the gate introduction earlier.
const SENSOR_CONTINUOUS_PRESENT_INTRODUCED: &str = "2026-04-02T00:00:00Z";

// ============================================================================
// Data structures
// ============================================================================

/// One encounter_split event from `day_log.jsonl`.
#[derive(Debug, Clone)]
struct SplitEvent {
    ts: DateTime<Utc>,
}

/// One continuous_mode_started event — a detector-task reset boundary.
/// Production re-initializes `sensor_continuous_present = false` inside the
/// detector task, so splits before a restart don't count toward the new run.
#[derive(Debug, Clone)]
struct RunStartEvent {
    ts: DateTime<Utc>,
}

/// One row from `~/.transcriptionapp/mmwave/{date}.csv`.
///
/// `presence` uses the `presence_debounced` column (1 = Present, 0 = Absent),
/// which is what production's `DebounceFsm` exposes to the detector loop.
#[derive(Debug, Clone, Copy)]
struct MmwaveReading {
    ts: DateTime<Utc>,
    present: bool,
}

/// Per-day cache loaded once, reused across every bundle under that day.
struct DayContext {
    splits: Vec<SplitEvent>,
    run_starts: Vec<RunStartEvent>,
    mmwave: Vec<MmwaveReading>,
    /// When neither day_log nor mmwave exists, we fall back to a
    /// bundle-only reconstruction that cannot distinguish true Present
    /// from true Absent at split time — in that case all checks get
    /// `sensor_continuous_present = false` (safe default, matches
    /// pre-patch CLI behavior).
    has_mmwave: bool,
}

#[derive(Debug, Default)]
struct Stats {
    bundles_found: usize,
    bundles_rewritten: usize,
    bundles_skipped_v2: usize,
    bundles_skipped_non_hybrid: usize,
    bundles_errored: usize,
    checks_total: usize,
    checks_gated: usize, // checks where reconstructed value was true
}

// ============================================================================
// Arg parsing
// ============================================================================

struct Args {
    archive: Option<PathBuf>,
    dry_run: bool,
    verbose: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        archive: None,
        dry_run: false,
        verbose: false,
    };
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--dry-run" => args.dry_run = true,
            "--verbose" | "-v" => args.verbose = true,
            "--archive" => {
                args.archive = iter.next().map(PathBuf::from);
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                print_usage();
                std::process::exit(1);
            }
        }
    }
    args
}

fn print_usage() {
    eprintln!("Replay Bundle Backfill Tool");
    eprintln!();
    eprintln!("Reconstructs sensor_continuous_present in historical replay_bundle.json");
    eprintln!("files from mmWave CSV + day_log.jsonl, and rewrites them as schema v2.");
    eprintln!();
    eprintln!("Usage: replay_bundle_backfill [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --archive PATH   Archive root (default: ~/.transcriptionapp/archive)");
    eprintln!("                   Can point at a year/month/day subdirectory to limit scope");
    eprintln!("  --dry-run        Show what would change without writing");
    eprintln!("  --verbose, -v    Print per-check decisions");
    eprintln!("  --help, -h       Show this help");
}

// ============================================================================
// Day context loaders
// ============================================================================

fn mmwave_root() -> PathBuf {
    dirs::home_dir()
        .expect("Could not resolve $HOME")
        .join(".transcriptionapp")
        .join("mmwave")
}

/// Find every `replay_bundle.json` under `root`, recursively.
fn find_bundles(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_bundles(root, &mut out);
    out.sort();
    out
}

fn walk_bundles(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_bundles(&path, out);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Match both the canonical bundle and merged-away sibling files
            // (see ReplayBundleBuilder::build_merged_and_reset).
            if name == "replay_bundle.json"
                || (name.starts_with("replay_bundle.merged_") && name.ends_with(".json"))
            {
                out.push(path);
            }
        }
    }
}

/// Extract `NaiveDate` from an archive path like
/// `.../archive/2026/03/20/{session_id}/replay_bundle.json`.
fn date_from_bundle_path(p: &Path) -> Option<NaiveDate> {
    let parts: Vec<&str> = p
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    // Look for YYYY/MM/DD in the path. The day directory is 3 levels above the bundle file:
    //   .../archive/YYYY/MM/DD/{session_id}/replay_bundle.json
    for i in 0..parts.len().saturating_sub(4) {
        let y = parts[i];
        let m = parts[i + 1];
        let d = parts[i + 2];
        if y.len() == 4 && m.len() == 2 && d.len() == 2 {
            if let (Ok(yi), Ok(mi), Ok(di)) = (y.parse::<i32>(), m.parse::<u32>(), d.parse::<u32>())
            {
                if let Some(nd) = NaiveDate::from_ymd_opt(yi, mi, di) {
                    return Some(nd);
                }
            }
        }
    }
    None
}

/// Load `encounter_split` and `continuous_mode_started` events from a day_log.
/// Returns them sorted by timestamp. Both are needed because production
/// resets `sensor_continuous_present` to false on each detector-task start.
fn load_day_log(day_dir: &Path) -> (Vec<SplitEvent>, Vec<RunStartEvent>) {
    let path = day_dir.join("day_log.jsonl");
    if !path.exists() {
        return (Vec::new(), Vec::new());
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let mut splits = Vec::new();
    let mut run_starts = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let event = v.get("event").and_then(Value::as_str).unwrap_or("");
        let ts_str = match v.get("ts").and_then(Value::as_str) {
            Some(s) => s,
            None => continue,
        };
        let ts = match DateTime::parse_from_rfc3339(ts_str) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => continue,
        };
        match event {
            "encounter_split" => splits.push(SplitEvent { ts }),
            "continuous_mode_started" => run_starts.push(RunStartEvent { ts }),
            _ => {}
        }
    }
    splits.sort_by_key(|s| s.ts);
    run_starts.sort_by_key(|r| r.ts);
    (splits, run_starts)
}

fn load_mmwave(date: NaiveDate) -> Vec<MmwaveReading> {
    let path = mmwave_root().join(format!(
        "{:04}-{:02}-{:02}.csv",
        date.year_from_date(),
        date.month_from_date(),
        date.day_from_date()
    ));
    if !path.exists() {
        return Vec::new();
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue; // header
        }
        // CSV columns: timestamp_utc,timestamp_local,presence_raw,presence_debounced,raw
        // The `raw` column may contain embedded commas inside quotes; we only
        // read the first four columns and ignore the rest.
        let mut it = line.splitn(5, ',');
        let ts_str = match it.next() {
            Some(s) => s,
            None => continue,
        };
        let _local = it.next();
        let _raw_pres = it.next();
        let debounced = match it.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let ts = match DateTime::parse_from_rfc3339(ts_str) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => continue,
        };
        let present = debounced == "1";
        out.push(MmwaveReading { ts, present });
    }
    out.sort_by_key(|r| r.ts);
    out
}

// chrono NaiveDate accessors are named `year()`, `month()`, `day()` — but
// `year()` clashes with the `Datelike` trait import convention, so we use
// wrapper fns for clarity and to make test mocks easier.
trait DateHelpers {
    fn year_from_date(&self) -> i32;
    fn month_from_date(&self) -> u32;
    fn day_from_date(&self) -> u32;
}

impl DateHelpers for NaiveDate {
    fn year_from_date(&self) -> i32 {
        use chrono::Datelike;
        self.year()
    }
    fn month_from_date(&self) -> u32 {
        use chrono::Datelike;
        self.month()
    }
    fn day_from_date(&self) -> u32 {
        use chrono::Datelike;
        self.day()
    }
}

fn load_day_context(day_dir: &Path, date: NaiveDate) -> DayContext {
    let (splits, run_starts) = load_day_log(day_dir);
    let mmwave = load_mmwave(date);
    let has_mmwave = !mmwave.is_empty();
    DayContext {
        splits,
        run_starts,
        mmwave,
        has_mmwave,
    }
}

/// Given `.../archive/YYYY/MM/DD/{session_id}/replay_bundle.json`, return
/// `.../archive/YYYY/MM/DD` — independent of how `--archive` was pointed,
/// so day_log.jsonl lookup always works.
fn day_dir_for_bundle(bundle_path: &Path) -> Option<PathBuf> {
    bundle_path.parent()?.parent().map(|p| p.to_path_buf())
}

// ============================================================================
// Reconstruction
// ============================================================================

/// Returns the debounced sensor state at time `t`, computed as "the most
/// recent reading at or before `t`". Returns `None` if no reading exists.
fn sensor_state_at(readings: &[MmwaveReading], t: DateTime<Utc>) -> Option<bool> {
    readings.iter().rev().find(|r| r.ts <= t).map(|r| r.present)
}

/// Returns `true` if any Present→Absent transition occurred in the half-open
/// interval `(start, end]`. Uses strict start so the transition at the split
/// boundary itself (if any) doesn't double-count against the new encounter.
fn had_present_to_absent_in(
    readings: &[MmwaveReading],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> bool {
    // Binary search would be nicer, but readings are small (~17k rows/day) and
    // we call this at most once per detection check — linear is fine.
    let mut prev: Option<bool> = None;
    for r in readings {
        if r.ts <= start {
            prev = Some(r.present);
            continue;
        }
        if r.ts > end {
            break;
        }
        if let Some(was_present) = prev {
            if was_present && !r.present {
                return true;
            }
        }
        prev = Some(r.present);
    }
    false
}

/// Reconstruct what `sensor_continuous_present` was at the moment this
/// detection check ran.
///
/// Matches production logic in `continuous_mode.rs`:
///   - Initial value: `false` (at each detector-task start, i.e. on each
///     `continuous_mode_started` event)
///   - Reset to `false` on any Present→Absent transition
///   - Set to `true` immediately after a successful split, iff the sensor was
///     Present at that moment (pre-patch production also guarded this with
///     `sensor_available`; we approximate with "there's a mmWave reading at
///     or before the split timestamp")
///
/// **Restart semantics**: if continuous mode was stopped and restarted
/// between the previous split and the current check, production re-initialized
/// the flag to `false`, and any splits before the restart don't count toward
/// the new run. We filter out splits that precede the most recent
/// `continuous_mode_started` event.
fn reconstruct_sensor_continuous_present(
    check_ts: DateTime<Utc>,
    day: &DayContext,
) -> bool {
    // Feature-introduction gate: pre-2026-04-01 production didn't have the
    // `sensor_continuous_present` field at all, so historical bundles from
    // before that date must stay at `false` even if the sensor data would
    // otherwise suggest the gate should apply.
    let introduced = DateTime::parse_from_rfc3339(SENSOR_CONTINUOUS_PRESENT_INTRODUCED)
        .expect("SENSOR_CONTINUOUS_PRESENT_INTRODUCED must be valid RFC3339")
        .with_timezone(&Utc);
    if check_ts < introduced {
        return false;
    }

    // Find the detector-task restart (if any) that is currently in effect
    // for this check. Splits before this restart are invisible — production
    // reset sensor_continuous_present=false inside `run_continuous_mode`.
    let run_start_ts = day
        .run_starts
        .iter()
        .rev()
        .find(|r| r.ts < check_ts)
        .map(|r| r.ts);

    // Find the most recent split that is both before `check_ts` AND within
    // the current run (i.e. after the most recent restart, if any).
    let prev_split = day
        .splits
        .iter()
        .rev()
        .find(|s| s.ts < check_ts && run_start_ts.map_or(true, |rs| s.ts >= rs));

    // No split in the current run → production's flag is still at its
    // initial `false`. First encounter of a fresh continuous-mode run.
    let Some(prev_split) = prev_split else {
        return false;
    };

    // If we have no mmWave data we cannot know what the sensor was doing at
    // split time. Safe default: `false` (same as pre-patch CLI behavior).
    if !day.has_mmwave {
        return false;
    }

    // Was the sensor Present immediately after the prior split? If we can't
    // find a reading at that time at all, the sensor probably wasn't
    // connected — treat as Absent.
    let Some(present_at_split) = sensor_state_at(&day.mmwave, prev_split.ts) else {
        return false;
    };
    if !present_at_split {
        return false;
    }

    // Sensor was Present at the split; production set sensor_continuous_present
    // to `true`. It stays true until a Present→Absent transition resets it.
    !had_present_to_absent_in(&day.mmwave, prev_split.ts, check_ts)
}

// ============================================================================
// Bundle rewriting
// ============================================================================

#[derive(Debug, Default)]
struct BundleStats {
    checks_total: usize,
    checks_gated: usize,
}

enum BundleOutcome {
    Rewritten(BundleStats),
    SkippedV2,
    SkippedNonHybrid,
}

fn process_bundle(
    path: &Path,
    day: &DayContext,
    dry_run: bool,
    verbose: bool,
) -> Result<BundleOutcome, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("read: {}", e))?;
    let mut bundle: Value = serde_json::from_str(&content).map_err(|e| format!("parse: {}", e))?;

    // Idempotency: already migrated.
    let schema_version = bundle
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if schema_version >= 2 {
        return Ok(BundleOutcome::SkippedV2);
    }

    // Only hybrid-mode bundles care about sensor_continuous_present. For
    // other modes we still bump schema_version so the file is marked up-to-date,
    // but we don't need to recompute anything.
    let is_hybrid = bundle
        .get("config")
        .and_then(|c| c.get("encounter_detection_mode"))
        .and_then(Value::as_str)
        == Some("hybrid");

    if !is_hybrid {
        // Non-hybrid: just bump the version and ensure loop_state has the three
        // new fields defaulted to false (matches the serde default path).
        if let Some(checks) = bundle
            .get_mut("detection_checks")
            .and_then(Value::as_array_mut)
        {
            for check in checks.iter_mut() {
                ensure_loop_state_fields(check, false);
            }
        }
        bundle["schema_version"] = json!(2);
        if !dry_run {
            atomic_write_json(path, &bundle)?;
        }
        return Ok(BundleOutcome::SkippedNonHybrid);
    }

    let mut stats = BundleStats::default();
    let checks = bundle
        .get_mut("detection_checks")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "missing detection_checks array".to_string())?;

    for (idx, check) in checks.iter_mut().enumerate() {
        stats.checks_total += 1;
        let ts_str = check
            .get("ts")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("check {} missing ts", idx))?;
        let ts = DateTime::parse_from_rfc3339(ts_str)
            .map_err(|e| format!("check {} bad ts: {}", idx, e))?
            .with_timezone(&Utc);
        let scp = reconstruct_sensor_continuous_present(ts, day);
        if scp {
            stats.checks_gated += 1;
        }
        if verbose {
            eprintln!(
                "    check[{}] ts={} → sensor_continuous_present={}",
                idx, ts_str, scp
            );
        }
        ensure_loop_state_fields(check, scp);
    }

    bundle["schema_version"] = json!(2);

    if !dry_run {
        atomic_write_json(path, &bundle)?;
    }
    Ok(BundleOutcome::Rewritten(stats))
}

/// Insert the three schema-v2 `loop_state` fields if missing. Never overwrites
/// an existing value — this makes the tool safe to run multiple times even if
/// someone has a partial v2 bundle in the wild.
fn ensure_loop_state_fields(check: &mut Value, sensor_continuous_present: bool) {
    let Some(ls) = check.get_mut("loop_state").and_then(Value::as_object_mut) else {
        return;
    };
    ls.entry("sensor_continuous_present")
        .or_insert(json!(sensor_continuous_present));
    // sensor_triggered: always safe to default to false because production
    // resets sensor_continuous_present to false in the same branch that sets
    // sensor_triggered to true (Present→Absent), so these two flags are never
    // simultaneously `true` inside evaluate_detection.
    ls.entry("sensor_triggered").or_insert(json!(false));
    // manual_triggered: manual triggers short-circuit the LLM path and never
    // produce a bundle check, so `false` is strictly correct here.
    ls.entry("manual_triggered").or_insert(json!(false));
}

/// UUID-suffixed temp file + rename. Mirrors the pattern in `profile-service`
/// `sessions.rs::atomic_write()` — if the process crashes mid-write we leave a
/// stray `.tmp-…` file but never a truncated bundle.
fn atomic_write_json(path: &Path, value: &Value) -> Result<(), String> {
    let pretty = serde_json::to_string_pretty(value).map_err(|e| format!("serialize: {}", e))?;
    let tmp = path.with_extension(format!("json.tmp.{}", uuid::Uuid::new_v4()));
    fs::write(&tmp, &pretty).map_err(|e| format!("write tmp: {}", e))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename: {}", e))?;
    Ok(())
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    let args = parse_args();
    let archive_root = args.archive.clone().unwrap_or_else(|| {
        transcription_app_lib::local_archive::get_archive_dir()
            .expect("Could not determine archive directory")
    });

    if !archive_root.exists() {
        eprintln!("Archive does not exist: {}", archive_root.display());
        std::process::exit(1);
    }

    println!("Archive: {}", archive_root.display());
    if args.dry_run {
        println!("Mode: DRY RUN (no files will be modified)");
    } else {
        println!("Mode: WRITE");
    }
    println!();

    let bundles = find_bundles(&archive_root);
    if bundles.is_empty() {
        println!("No replay_bundle.json files found.");
        return;
    }

    let mut stats = Stats::default();
    stats.bundles_found = bundles.len();
    println!("Found {} replay_bundle.json files", bundles.len());
    println!();

    // Per-day cache. Loading mmwave CSV + day_log repeatedly for the same
    // day would be wasteful — most days have multiple bundles.
    let mut day_cache: HashMap<NaiveDate, DayContext> = HashMap::new();

    for bundle_path in &bundles {
        let date = match date_from_bundle_path(bundle_path) {
            Some(d) => d,
            None => {
                eprintln!(
                    "  SKIP (cannot extract date from path): {}",
                    bundle_path.display()
                );
                stats.bundles_errored += 1;
                continue;
            }
        };
        let day_dir = match day_dir_for_bundle(bundle_path) {
            Some(d) => d,
            None => {
                eprintln!(
                    "  SKIP (cannot derive day directory): {}",
                    bundle_path.display()
                );
                stats.bundles_errored += 1;
                continue;
            }
        };
        let day = day_cache
            .entry(date)
            .or_insert_with(|| load_day_context(&day_dir, date));

        match process_bundle(bundle_path, day, args.dry_run, args.verbose) {
            Ok(BundleOutcome::Rewritten(bs)) => {
                stats.bundles_rewritten += 1;
                stats.checks_total += bs.checks_total;
                stats.checks_gated += bs.checks_gated;
                if args.verbose || !args.dry_run {
                    let relative = bundle_path
                        .strip_prefix(&archive_root)
                        .unwrap_or(bundle_path);
                    println!(
                        "  {} ({} checks, {} with gate on)",
                        relative.display(),
                        bs.checks_total,
                        bs.checks_gated
                    );
                }
            }
            Ok(BundleOutcome::SkippedV2) => {
                stats.bundles_skipped_v2 += 1;
            }
            Ok(BundleOutcome::SkippedNonHybrid) => {
                stats.bundles_skipped_non_hybrid += 1;
                stats.bundles_rewritten += 1;
            }
            Err(e) => {
                stats.bundles_errored += 1;
                eprintln!("  ERROR {}: {}", bundle_path.display(), e);
            }
        }
    }

    // Also print mmwave-data availability per day that appears in the archive.
    // Days without CSV coverage get conservative `false` defaults, which is
    // worth calling out explicitly.
    let mut days_without_mmwave = 0;
    for (date, ctx) in &day_cache {
        if !ctx.has_mmwave {
            days_without_mmwave += 1;
            if args.verbose {
                eprintln!("  (day {} had no mmwave CSV — defaulted to false)", date);
            }
        }
    }

    println!();
    println!("───────────────────────────────────────");
    println!("Bundles found:          {}", stats.bundles_found);
    println!("Rewritten (hybrid):     {}", stats.bundles_rewritten - stats.bundles_skipped_non_hybrid);
    println!("Rewritten (non-hybrid): {}", stats.bundles_skipped_non_hybrid);
    println!("Already v2 (skipped):   {}", stats.bundles_skipped_v2);
    println!("Errored:                {}", stats.bundles_errored);
    println!("Detection checks:       {} total, {} now gate-on", stats.checks_total, stats.checks_gated);
    if days_without_mmwave > 0 {
        println!(
            "Days without mmwave:    {} (those checks defaulted to false)",
            days_without_mmwave
        );
    }
    if args.dry_run {
        println!();
        println!("Dry run complete. Re-run without --dry-run to apply.");
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    fn reading(s: &str, present: bool) -> MmwaveReading {
        MmwaveReading {
            ts: ts(s),
            present,
        }
    }

    #[test]
    fn first_check_no_prior_split_returns_false() {
        let day = DayContext {
            splits: vec![],
            run_starts: vec![],
            mmwave: vec![reading("2026-03-20T10:00:00Z", true)],
            has_mmwave: true,
        };
        // Production starts with sensor_continuous_present=false and only flips
        // it to true after a successful split. No prior split → false.
        assert!(!reconstruct_sensor_continuous_present(
            ts("2026-03-20T10:05:00Z"),
            &day
        ));
    }

    #[test]
    fn after_split_with_continuous_present_returns_true() {
        // Uses a post-feature-introduction date (2026-04-05 > 2026-04-01)
        // so the gate is active; the pre-feature cutoff test lives separately.
        let day = DayContext {
            splits: vec![SplitEvent {
                ts: ts("2026-04-05T10:00:00Z"),
            }],
            run_starts: vec![],
            mmwave: vec![
                reading("2026-04-05T09:59:30Z", true),
                reading("2026-04-05T10:00:30Z", true),
                reading("2026-04-05T10:05:00Z", true),
                reading("2026-04-05T10:10:00Z", true),
            ],
            has_mmwave: true,
        };
        // Split at 10:00, sensor present the entire time through 10:15,
        // check at 10:15 → gate should be ON.
        assert!(reconstruct_sensor_continuous_present(
            ts("2026-04-05T10:15:00Z"),
            &day
        ));
    }

    #[test]
    fn after_split_with_absent_transition_returns_false() {
        let day = DayContext {
            splits: vec![SplitEvent {
                ts: ts("2026-03-20T10:00:00Z"),
            }],
            run_starts: vec![],
            mmwave: vec![
                reading("2026-03-20T09:59:30Z", true),
                reading("2026-03-20T10:00:30Z", true),
                // Sensor departs at 10:07 — this breaks the continuous-present streak.
                reading("2026-03-20T10:07:00Z", false),
                reading("2026-03-20T10:10:00Z", true), // returned, but streak broken
            ],
            has_mmwave: true,
        };
        assert!(!reconstruct_sensor_continuous_present(
            ts("2026-03-20T10:15:00Z"),
            &day
        ));
    }

    #[test]
    fn after_split_when_sensor_absent_at_split_returns_false() {
        // Production sets sensor_continuous_present = (sensor_available &&
        // prev_sensor_state == Present) at split time. If sensor was Absent
        // at the split moment, the flag was never set to true.
        let day = DayContext {
            splits: vec![SplitEvent {
                ts: ts("2026-03-20T10:00:00Z"),
            }],
            run_starts: vec![],
            mmwave: vec![
                reading("2026-03-20T09:59:30Z", false),
                reading("2026-03-20T10:00:30Z", true), // returned AFTER the split
                reading("2026-03-20T10:05:00Z", true),
            ],
            has_mmwave: true,
        };
        assert!(!reconstruct_sensor_continuous_present(
            ts("2026-03-20T10:10:00Z"),
            &day
        ));
    }

    #[test]
    fn multiple_splits_uses_most_recent() {
        // Uses post-feature-introduction dates so the gate is active.
        let day = DayContext {
            splits: vec![
                SplitEvent {
                    ts: ts("2026-04-05T09:00:00Z"),
                },
                SplitEvent {
                    ts: ts("2026-04-05T10:00:00Z"),
                },
            ],
            run_starts: vec![],
            mmwave: vec![
                // Absent event at 09:30 — would break the first split's streak
                // but is irrelevant to the second split's streak.
                reading("2026-04-05T09:00:30Z", true),
                reading("2026-04-05T09:30:00Z", false),
                reading("2026-04-05T09:45:00Z", true),
                reading("2026-04-05T10:00:30Z", true),
                reading("2026-04-05T10:10:00Z", true),
            ],
            has_mmwave: true,
        };
        // Check at 10:15 should only care about what happened after 10:00
        assert!(reconstruct_sensor_continuous_present(
            ts("2026-04-05T10:15:00Z"),
            &day
        ));
    }

    #[test]
    fn pre_feature_introduction_always_returns_false() {
        // Regression: the sensor_continuous_present gate shipped on 2026-04-01.
        // Bundles captured before that date were produced by code that didn't
        // have the field at all — their LLM splits at 0.85-0.95 confidence
        // were allowed because no 0.99 gate existed. The backfill must not
        // retroactively apply the 2026-04-01 feature to older data.
        let day = DayContext {
            splits: vec![SplitEvent {
                ts: ts("2026-03-17T17:34:00Z"),
            }],
            run_starts: vec![RunStartEvent {
                ts: ts("2026-03-17T17:09:48Z"),
            }],
            mmwave: vec![
                reading("2026-03-17T17:33:50Z", true),
                reading("2026-03-17T17:50:00Z", true),
            ],
            has_mmwave: true,
        };
        // Even though sensor was continuously Present across a prior split,
        // this is a pre-feature date, so the gate must stay off.
        assert!(!reconstruct_sensor_continuous_present(
            ts("2026-03-17T17:46:35Z"),
            &day
        ));
    }

    #[test]
    fn post_feature_introduction_applies_gate_normally() {
        // Same scenario but after the feature introduction date: the gate
        // should apply as expected.
        let day = DayContext {
            splits: vec![SplitEvent {
                ts: ts("2026-04-02T17:34:00Z"),
            }],
            run_starts: vec![RunStartEvent {
                ts: ts("2026-04-02T17:09:48Z"),
            }],
            mmwave: vec![
                reading("2026-04-02T17:33:50Z", true),
                reading("2026-04-02T17:50:00Z", true),
            ],
            has_mmwave: true,
        };
        assert!(reconstruct_sensor_continuous_present(
            ts("2026-04-02T17:46:35Z"),
            &day
        ));
    }

    #[test]
    fn continuous_mode_restart_resets_flag() {
        // Regression: on 2026-04-10 at 13:41:27 continuous mode was restarted
        // between a 13:40:53 manual split and a 13:45:30 new encounter's first
        // detection check. Production's sensor_continuous_present was re-
        // initialized to false inside the detector task at restart time, so
        // the pre-restart split doesn't count toward the new run.
        let day = DayContext {
            splits: vec![SplitEvent {
                ts: ts("2026-04-10T13:40:53Z"),
            }],
            run_starts: vec![RunStartEvent {
                ts: ts("2026-04-10T13:41:27Z"),
            }],
            mmwave: vec![
                reading("2026-04-10T13:40:00Z", true),
                reading("2026-04-10T13:45:00Z", true),
                reading("2026-04-10T13:46:00Z", true),
            ],
            has_mmwave: true,
        };
        // Without the restart fix this would be true (sensor was Present
        // at the 13:40:53 split and stayed Present through 13:45:30). With
        // the fix we correctly see that the restart at 13:41:27 invalidated
        // the pre-restart split, so the flag is false.
        assert!(!reconstruct_sensor_continuous_present(
            ts("2026-04-10T13:45:30Z"),
            &day
        ));
    }

    #[test]
    fn split_after_restart_still_counts() {
        // Same day as above but with a post-restart split: the restart happened
        // at 13:41:27, then a split happened at 13:50:00 with sensor Present,
        // then a check at 13:55:00. The post-restart split DOES count.
        let day = DayContext {
            splits: vec![
                SplitEvent {
                    ts: ts("2026-04-10T13:40:53Z"),
                },
                SplitEvent {
                    ts: ts("2026-04-10T13:50:00Z"),
                },
            ],
            run_starts: vec![RunStartEvent {
                ts: ts("2026-04-10T13:41:27Z"),
            }],
            mmwave: vec![reading("2026-04-10T13:45:00Z", true)],
            has_mmwave: true,
        };
        assert!(reconstruct_sensor_continuous_present(
            ts("2026-04-10T13:55:00Z"),
            &day
        ));
    }

    #[test]
    fn no_mmwave_data_returns_false() {
        let day = DayContext {
            splits: vec![SplitEvent {
                ts: ts("2026-03-20T10:00:00Z"),
            }],
            run_starts: vec![],
            mmwave: vec![],
            has_mmwave: false,
        };
        // Without CSV we can't verify the sensor was Present at split time →
        // default to false (same as pre-patch CLI behavior).
        assert!(!reconstruct_sensor_continuous_present(
            ts("2026-03-20T10:10:00Z"),
            &day
        ));
    }

    #[test]
    fn had_present_to_absent_strictly_after_start() {
        let readings = vec![
            reading("2026-03-20T10:00:00Z", true),
            reading("2026-03-20T10:05:00Z", false),
        ];
        // Interval (10:00, 10:10] — the 10:05 transition falls inside
        assert!(had_present_to_absent_in(
            &readings,
            ts("2026-03-20T10:00:00Z"),
            ts("2026-03-20T10:10:00Z")
        ));
        // Interval (10:06, 10:10] — no transition
        assert!(!had_present_to_absent_in(
            &readings,
            ts("2026-03-20T10:06:00Z"),
            ts("2026-03-20T10:10:00Z")
        ));
    }

    #[test]
    fn sensor_state_at_uses_latest_before_t() {
        let readings = vec![
            reading("2026-03-20T10:00:00Z", true),
            reading("2026-03-20T10:05:00Z", false),
            reading("2026-03-20T10:10:00Z", true),
        ];
        assert_eq!(sensor_state_at(&readings, ts("2026-03-20T09:59:00Z")), None);
        assert_eq!(
            sensor_state_at(&readings, ts("2026-03-20T10:00:00Z")),
            Some(true)
        );
        assert_eq!(
            sensor_state_at(&readings, ts("2026-03-20T10:04:59Z")),
            Some(true)
        );
        assert_eq!(
            sensor_state_at(&readings, ts("2026-03-20T10:05:00Z")),
            Some(false)
        );
        assert_eq!(
            sensor_state_at(&readings, ts("2026-03-20T10:20:00Z")),
            Some(true)
        );
    }

    #[test]
    fn ensure_loop_state_fields_preserves_existing_values() {
        // If someone has a partially-migrated bundle where, say,
        // sensor_continuous_present=true has already been set but the others
        // haven't, rerunning the tool must not stomp the existing value.
        let mut check = json!({
            "loop_state": {
                "consecutive_failures": 0,
                "merge_back_count": 0,
                "buffer_age_secs": 600.0,
                "sensor_continuous_present": true,
            }
        });
        ensure_loop_state_fields(&mut check, false); // backfill says false
        let ls = check["loop_state"].as_object().unwrap();
        assert_eq!(ls["sensor_continuous_present"], json!(true)); // preserved
        assert_eq!(ls["sensor_triggered"], json!(false));
        assert_eq!(ls["manual_triggered"], json!(false));
    }

    #[test]
    fn ensure_loop_state_fields_adds_all_three_on_fresh_bundle() {
        let mut check = json!({
            "loop_state": {
                "consecutive_failures": 0,
                "merge_back_count": 0,
                "buffer_age_secs": 600.0,
            }
        });
        ensure_loop_state_fields(&mut check, true);
        let ls = check["loop_state"].as_object().unwrap();
        assert_eq!(ls["sensor_continuous_present"], json!(true));
        assert_eq!(ls["sensor_triggered"], json!(false));
        assert_eq!(ls["manual_triggered"], json!(false));
    }

    #[test]
    fn date_from_bundle_path_extracts_ymd() {
        let p = PathBuf::from(
            "/Users/test/.transcriptionapp/archive/2026/03/20/abc123/replay_bundle.json",
        );
        assert_eq!(
            date_from_bundle_path(&p),
            Some(NaiveDate::from_ymd_opt(2026, 3, 20).unwrap())
        );
    }

    #[test]
    fn date_from_bundle_path_rejects_non_date_segments() {
        let p = PathBuf::from("/tmp/not/a/date/path/replay_bundle.json");
        assert_eq!(date_from_bundle_path(&p), None);
    }

    #[test]
    fn walk_bundles_includes_merged_siblings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = dir.path().join("2026").join("04").join("15").join("session-id");
        fs::create_dir_all(&session).unwrap();
        fs::write(session.join("replay_bundle.json"), "{}").unwrap();
        fs::write(session.join("replay_bundle.merged_abc12345.json"), "{}").unwrap();
        fs::write(session.join("metadata.json"), "{}").unwrap();
        fs::write(session.join("transcript.txt"), "").unwrap();

        let bundles = find_bundles(dir.path());
        assert_eq!(bundles.len(), 2, "should find canonical + merged sibling");
        let names: Vec<String> = bundles
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        assert!(names.contains(&"replay_bundle.json".to_string()));
        assert!(names.iter().any(|n| n == "replay_bundle.merged_abc12345.json"));
    }
}
