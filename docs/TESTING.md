# Testing Architecture

This document describes the AMI Assist test infrastructure: what's tested, where, how to run things, and how to extend the test suite.

## At a glance

| Surface | Files | Tests | Runner |
|---------|-------|-------|--------|
| Rust backend (lib) | ~86 | 1,076 | `cd tauri-app/src-tauri && cargo test --lib` |
| Rust CLI tool tests | 4 binaries | ~46 inline | `cargo test --bins` |
| Profile service | 7 | 46 | `cd profile-service && cargo test` |
| Frontend (React + TS) | 31 | 585 | `cd tauri-app && pnpm test:run` |
| E2E (live services) | 1 file, 11 tests | All `#[ignore]` | `./scripts/preflight.sh --full` |
| Replay regressions | 6 CLIs | Run against archive | See "Replay tools" below |

## Test layers

### Layer 1: Unit tests
Standard `#[cfg(test)] mod tests` in each Rust module, plus `*.test.tsx` for React. These cover pure functions, data validation, parsers, and hook behaviors.

### Layer 2: Integration tests
- `command_tests.rs` — Tauri command handlers
- `pipeline_tests.rs` — audio pipeline integration (some require ONNX)
- `profile-service/tests/` — HTTP endpoint tests via `axum::Router` + `tempfile::tempdir()`

### Layer 3: Stress + soak
- `stress_tests.rs` — high-load scenarios (always run)
- `soak_tests.rs` — long-running stability (`#[ignore]`, run with `pnpm soak:1h` for the 1-hour suite)

### Layer 4: E2E (live services)
- `e2e_tests.rs` — 5-layer integration suite, all `#[ignore]`. Run via `./scripts/preflight.sh`:
  - Layer 1: STT Router health/streaming
  - Layer 2: LLM Router SOAP/detection
  - Layer 3: Local archive
  - Layer 4: Session mode end-to-end
  - Layer 5: Continuous mode end-to-end

### Layer 5: Replay regressions (offline + online)
The most powerful layer — uses real archived production data as test inputs.
- **Offline** (deterministic, no LLM): `detection_replay_cli` against archived bundles.
- **Online** (live LLM, non-deterministic): `merge_replay_cli`, `clinical_replay_cli`, `multi_patient_replay_cli`, `multi_patient_split_replay_cli`.
- **Benchmarks** (curated test cases): `benchmark_runner` against `tests/fixtures/benchmarks/*.json`.

### Layer 6: Golden day regression
End-to-end labeled day from the archive. Compares production's actual outcomes to documented "correct" outcomes for one fully reviewed clinic day.

## Replay tools

All replay tools live in `tauri-app/src-tauri/tools/` and are registered as `[[bin]]` entries in `Cargo.toml`. Each follows a consistent CLI pattern:

```
cargo run --bin <tool> -- [PATH | --all] [--trials N] [--fail-on-mismatch] [--threshold PCT] [--mismatches]
```

| Tool | What it replays | LLM required? | Default threshold |
|------|-----------------|---------------|-------------------|
| `detection_replay_cli` | `evaluate_detection()` pure function | No (deterministic) | 99.0% |
| `merge_replay_cli` | Merge-check LLM calls | Yes | 75.0% |
| `clinical_replay_cli` | Clinical content check LLM calls | Yes | 90.0% |
| `multi_patient_replay_cli` | Multi-patient detection LLM calls | Yes | 80.0% |
| `multi_patient_split_replay_cli` | Multi-patient split (line_index) LLM calls | Yes | 70.0% (±2 lines) |
| `benchmark_runner` | Curated TC fixtures from docs/benchmarks | Yes | per-fixture |
| `labeled_regression_cli` | Per-session labeled outputs vs production | No (offline) | n/a (per-check) |
| `golden_day_cli` | Full clinic-day labeled corpus integrity | No (offline) | n/a (all-or-nothing) |
| `bootstrap_labels` | Generate label fixtures from production billing | No (offline) | n/a (creates labels) |

### Why thresholds vary
LLM responses at temp=0.3 have a documented ~40% flip rate on borderline cases. Each task has a different difficulty distribution; thresholds reflect what's achievable with `--trials 3` majority voting on the labeled corpus.

## Performance summary (ops observability)

Written automatically at `continuous_mode_stopped` to `archive/YYYY/MM/DD/performance_summary.json`. Aggregates the day's `pipeline_log.jsonl` + `day_log.jsonl` data into per-step percentiles (p50/p90/p99/max), failure counts, and — for LLM steps migrated to `generate_timed` / `generate_vision_timed` — `total_scheduling_ms` / `total_network_ms` / `peak_concurrent` / `retried_call_count`.

Useful for attributing tail latencies without parsing 10+ per-session files. Verify with the ignored integration test:

```bash
cargo test --lib performance_summary::tests::show_real_day -- --ignored --nocapture
```

The file format is stable and machine-readable; external dashboards can tail it across clinic days without coupling to session-directory structure.

### Why offline replay matters
`detection_replay_cli` is the only fully-deterministic regression gate. It runs in <30s against ~2,500 historical decisions and catches **any logic regression in `evaluate_detection()`** — the central decision function. It's wired into `preflight.sh` as Layer 6.

## Adding a new benchmark task

1. **Define the fixture** at `tauri-app/src-tauri/tests/fixtures/benchmarks/<task_name>.json`:
   ```json
   {
     "task": "<task_name>",
     "model": "fast-model",
     "targets": {
       "overall_accuracy_pct": 90.0
     },
     "test_cases": [
       {
         "id": "TC-1",
         "name": "...",
         "difficulty": "easy",
         "input": "...",
         "expected_clinical": true
       }
     ]
   }
   ```
2. **Wire the task into `tools/benchmark_runner.rs`** — add a `match` arm in `main()` that calls a new `run_<task_name>(...)` function.
3. **Test it** with `cargo run --bin benchmark_runner -- <task_name> --trials 3`.

## Growing the labeled corpus

Two paths:

**Bootstrap from production** — fastest, lowest-effort:
```bash
cd tauri-app/src-tauri
cargo run --bin bootstrap_labels -- 2026-04-16
```
This walks the day's archive, reads each session's `metadata.json` + `billing.json`, and writes a label file per session that asserts the current production output is correct. After bootstrap, manual review can downgrade individual assertions for known errors.

**Manual labeling** — higher effort, captures genuine ground truth:
1. Open the History window, review the session's transcript + SOAP + billing
2. Hand-write `tauri-app/src-tauri/tests/fixtures/labels/{date}_{short_id}.json` per the schema in `tests/fixtures/labels/README.md`
3. Use `clinical_correct: false` or `billing_codes_expected: [different from prod]` to lock in the corrected answer

Either way, the next `labeled_regression_cli --all` run will report production divergence from the labels.

## Adding a new replay CLI

The pattern from `merge_replay_cli.rs`:

1. Create `tools/<task>_replay_cli.rs`.
2. Iterate `find_replay_bundles(...)` from existing CLIs.
3. Read each bundle's archived LLM call (e.g., `bundle.merge_check`, `bundle.clinical_check`, `bundle.multi_patient_detections`).
4. Re-issue the captured prompts through `LLMClient::generate(...)`.
5. Parse using the production parser (`parse_merge_check`, `parse_clinical_content_check`, etc.).
6. Compare to the archived parsed result; aggregate match/mismatch.
7. Register as `[[bin]]` in `Cargo.toml`.

## Ground truth labels

Located at `tauri-app/src-tauri/tests/fixtures/labels/`. JSON schema:

```json
{
  "session_id": "...",
  "date": "2026-04-15",
  "labeled_at": "2026-04-16T...",
  "labeled_by": "Dr Z",
  "labels": {
    "split_correct": true,
    "merge_correct": true,
    "clinical_correct": true,
    "patient_count_correct": true,
    "billing_codes_expected": ["A007A", "Q310A"],
    "diagnostic_code_expected": "311",
    "notes": "Headache encounter, clean split"
  }
}
```

The canonical schema reference lives at `tauri-app/src-tauri/tests/fixtures/labels/README.md`. The `_expected` suffix on `billing_codes_expected` and `diagnostic_code_expected` is intentional — it distinguishes the expectation (what should be true) from the production value (what currently is true).

Run the labeled regression with `cargo run --bin labeled_regression_cli -- --all --fail-on-regression`.

## Test counts (target / current)

| Surface | Target | Current |
|---------|--------|---------|
| Rust unit + integration | 1,000+ | 1,076 |
| Frontend hook + component | 600+ | 585 |
| Profile service | 50+ | 46 |
| Replay corpus (bundles) | 200+ | 192 |
| Labeled bundles | 30+ | 68 (6 days: 04-08, 04-09, 04-10, 04-13, 04-14, 04-15) |
| Benchmark test cases | 30+ | 21 (5 tasks: clinical, detection, merge, multi-patient detection, multi-patient split) |
| Replay regression CLIs | 5 | 6 (detection + merge + clinical + multi-patient + multi-patient-split + golden-day) |
| Preflight layers | 7 | 7 (1-5 E2E + 6 detection replay + 7 golden day) |
| Bundle schema version | — | v3 (added MultiPatientSplitDecision capture) |

## Removed test infrastructure (Apr 2026)

These tiers were deleted as dormant/abandoned:
- `e2e/` (WebdriverIO) — never updated after the "AMI Assist" rename
- `tests/visual/` (Playwright) — no snapshots ever generated
- `stryker.config.mjs` (mutation testing) — never completed a run

If you want to re-introduce browser automation, recommendation: use the existing replay tools (which test the actual LLM/data flows) over re-adding WebdriverIO/Playwright (which test the frontend in isolation from the real production data).

## Common workflows

### Before a release
```bash
./scripts/preflight.sh --full          # Full E2E (~30s)
cd src-tauri && cargo test --lib       # All Rust unit tests
cd .. && pnpm test:run                  # All frontend tests

# Optional but recommended:
cd src-tauri && cargo run --bin merge_replay_cli -- --all --trials 3 --fail-on-mismatch
cd src-tauri && cargo run --bin clinical_replay_cli -- --all --trials 3 --fail-on-mismatch
cd src-tauri && cargo run --bin benchmark_runner -- --all --trials 3 --fail-on-regression
```

### After changing an LLM prompt
The `encounter_detection::tests::test_replay_day_py_has_current_detection_prompt` test will fail at build time if you forget to mirror the change in `scripts/replay_day.py`.

For prompts captured in replay bundles: run the relevant replay CLI to verify accuracy hasn't regressed.

### After changing the rule engine
```bash
cd src-tauri && cargo test --lib billing
cd src-tauri && cargo run --bin benchmark_runner -- --all --fail-on-regression
cd src-tauri && cargo run --bin labeled_regression_cli -- --all --fail-on-regression
```

### A/B testing a model or prompt change end-to-end on real audio
Use the Python orchestrator at `tauri-app/scripts/replay_day.py`:
```bash
cd tauri-app
python3 scripts/replay_day.py transcribe 2026-04-15
python3 scripts/replay_day.py replay 2026-04-15 default       # baseline
python3 scripts/replay_day.py replay 2026-04-15 soap_alt      # alternative model
python3 scripts/replay_day.py compare 2026-04-15
```
Reports encounter counts, SOAP item counts, and failure rates side-by-side. Caches transcripts at `/tmp/replay_<date>/` so repeated `replay` runs are cheap.

### Investigating a regression
```bash
# Show only mismatched bundles
cd src-tauri && cargo run --bin detection_replay_cli -- --all --mismatches

# What-if analysis (try a different threshold)
cd src-tauri && cargo run --bin detection_replay_cli -- --all --override hybrid_confirm_window_secs=120
```
