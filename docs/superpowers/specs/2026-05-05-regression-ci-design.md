# Regression Corpus Wired Into Mandatory CI

**Date:** 2026-05-05
**Author:** Claude (paired with Arash)
**Status:** Approved for implementation (in-session brainstorm)
**Related:** v0.10.70 release-pipeline hardening, ADR-0028 (replay logging), `tauri-app/scripts/preflight.sh`, `tests/fixtures/labels/`

## Goal

Make every PR run the offline regression corpus and block merge on regressions. v0.10.70 was the first ratchet — gating *releases* on `ort_smoke`. This is the second ratchet — gating *PRs* on `golden_day_cli + labeled_regression_cli + detection_replay_cli + harness_per_encounter`.

## Context

The repo has six production-grade regression tools today:

1. **`harness_per_encounter`** (~10 seed bundles, in-repo) — runs in `cargo test --all-features`. Already mandatory, but invisible.
2. **`detection_replay_cli`** — pure-function replay of `evaluate_detection()` over `replay_bundle.json` files. No LLM.
3. **`labeled_regression_cli`** — compares production billing/dx/clinical output to `tests/fixtures/labels/*.json` (135 files, ~327 checks across 124 labels at v0.10.68).
4. **`golden_day_cli`** — structural integrity: every archived session has a label, no spurious splits, no missing sessions.
5. **`benchmark_runner`** — *live LLM Router* required. Out of scope.
6. **`merge_replay_cli` / `clinical_replay_cli` / `multi_patient_replay_cli`** — re-issue archived LLM calls. Live LLM. Out of scope.

The labels are in repo, but the `replay_bundle.json` files they reference live in `~/.transcriptionapp/archive/` (PHI). So the gate cannot run on GitHub-hosted runners — it needs read access to the production archive on the MacBook.

## Decisions (from brainstorm)

| Dimension | Choice |
|-----------|--------|
| Substrate | **Self-hosted runner on the MacBook** (100.119.83.76). Reads existing `~/.transcriptionapp/archive/`. |
| Pass/fail policy | **Pinned per-label expectations** — each label file declares which checks are currently expected to fail. |
| Schema granularity | **Per-check** (`expected_failures: ["billing_codes", "diagnostic_code"]`). Ratchet tightens by removing entries. |
| Scope of CI gate | `labeled_regression_cli` + `detection_replay_cli` + `harness_per_encounter`. (`golden_day_cli` was originally in scope but moved to `--full` daily preflight only — see "Layer 7 carve-out" below.) |
| Wiring style | **Single script** (`scripts/preflight.sh --regression`) invoked from one CI job. Mirrors existing preflight pattern; one local command reproduces CI failure. |

## Schema change

`LabelData` (in `src/feedback_to_label.rs`) gains a new optional field:

```rust
/// Names of checks (e.g. "billing_codes", "diagnostic_code", "clinical_classification")
/// that are CURRENTLY expected to fail. The regression CLI counts these as
/// "expected failures" rather than regressions. Removing an entry tightens the
/// gate — the next run that produces the same failure will be a regression.
#[serde(default)]
pub expected_failures: Option<Vec<String>>,
```

Stable check names emitted by `labeled_regression_cli`:

| Name | Source field | Notes |
|------|-------------|-------|
| `clinical_classification` | `clinical_correct` | One per label. |
| `billing_codes` | `billing_codes_expected` | Subset test — single check even when the list has multiple codes. |
| `billing_codes_unexpected:{CODE}` | `billing_codes_unexpected` | One per code in the unexpected list. |
| `billing_quantity:{CODE}` | `billing_quantity_expected` | One per code in the quantity map. |
| `diagnostic_code` | `diagnostic_code_expected` | Honors `diagnostic_code_acceptable` for synonyms. |
| `billing_json_missing` | (synthetic) | Fires when label expects billing but `billing.json` doesn't exist. |
| `date_parse` | (synthetic) | Fires on malformed `date`. |

## CLI semantic change (`labeled_regression_cli`)

Two new flags:

- `--bootstrap-expected-failures` — runs all checks, records the names of every fired failure into `label.expected_failures`, writes the file back. One-time migration to seed the baseline at the current corpus state. Idempotent (deterministic order, sorted, deduped).
- `--fail-on-regression` (existing flag, behavior tightened): exit `2` only when a non-expected check fails. Expected failures (in `expected_failures`) and "drift" (entry was expected to fail but actually passed) are surfaced but exit 0.

Drift semantic: when a check listed in `expected_failures` actually passes, it's surfaced as `↑ tighten {name}` so the clinician can drop it from the label and ratchet the gate. Drift is **informational**, never blocking.

## Preflight script extension

`tauri-app/scripts/preflight.sh` gains:

- **Layer 9** — `labeled_regression_cli --all --fail-on-regression`.
- **`--regression` mode** — runs Layers 6, 8, 9 (the code-regression layers). Skips connectivity layers 1–5 (which need live STT/LLM) and Layer 7 (which is clinical maintenance — see carve-out below).

Existing behavior preserved: default quick mode unchanged, `--full` runs all 9 layers.

### Layer 7 carve-out (golden_day_cli)

Originally part of the gate. Moved to `--full` only after discovering pre-existing corpus drift on Room 6's local archive that's unrelated to code: a labeled-but-known-missing session (`gary-stewart-missing.json`), a cross-room session that hasn't synced yet, and three test-artifact directories with all-zero UUIDs. Layer 7 enforces *corpus completeness* (every archived session has a label, every label points at an archived session). That state changes for clinical reasons (cross-room sync, manual labelling, test cleanup) — gating PRs on it would block merges for reasons unrelated to the code change.

Layer 9 (`labeled_regression_cli`) gives us per-check code-regression coverage with the `expected_failures` ratchet — that's the load-bearing PR gate. Layer 7 stays in `preflight.sh --full` so daily clinic prep continues to surface corpus drift to Arash.

A future PR can add `expected_missing` (per-label) and `expected_count_drift` (per-day) baselines to `golden_day_cli`, mirroring the Layer 9 ratchet, and re-include it in `--regression`.

## CI wiring

New job in `.github/workflows/ci.yml`:

```yaml
regression-corpus:
  name: Regression Corpus
  runs-on: [self-hosted, macOS, ami-ci]
  defaults:
    run:
      working-directory: tauri-app
  steps:
    - uses: actions/checkout@v4
    - run: ./scripts/preflight.sh --regression
```

Trigger: same as the other jobs (push to main, every PR).

The runner uses tag `ami-ci` to disambiguate from any future MacBook-hosted runners. Bring-up doc: `docs/regression-ci-runner-setup.md`.

## Migration plan

1. Land the schema field + CLI changes + tests.
2. Run `cargo run --bin labeled_regression_cli -- --all --bootstrap-expected-failures` against the local archive on Room 6 (this machine has 304 archived `replay_bundle.json` files). This populates the initial `expected_failures` arrays in the 124 label files at the current corpus state.
3. Run `cargo run --bin labeled_regression_cli -- --all --fail-on-regression` — must exit 0 (every existing failure now expected).
4. Run `./scripts/preflight.sh --regression` — must exit 0.
5. Land preflight + CI changes.
6. (Out-of-band, by Arash) Register the self-hosted runner on the MacBook.
7. (Optional follow-up PR) Tighten the gate by sweeping `expected_failures` entries that turned to drift.

## Out of scope

- `benchmark_runner` (needs live LLM Router; not part of the offline gate).
- The replay-CLIs that re-issue archived LLM calls (`merge_replay_cli`, etc.) — same reason.
- Self-hosted runner agent registration on the MacBook (irreversible-ish shared-infra change; documented as manual ops step).
- Mid-PR auto-tightening (drift becoming hard-fail). Day-1 drift is informational only.

## Risk + non-goals

- **Risk: the corpus has known mismatches.** Mitigated by bootstrapping `expected_failures` against current state, so day-1 the gate is "no new regressions" not "everything passes."
- **Risk: archive on the runner can drift from production.** Mitigated by the runner being the production archive itself (the MacBook also runs the profile service, so its archive is authoritative — Room 6's local archive is just a syncing client).
- **Risk: CI dies if MacBook is offline.** Acceptable for now — the MacBook is the always-on server (runs LLM Router, STT Router, profile service, Medplum). If it's offline, the clinic is offline, so PR gating is the least of our worries.
- **Non-goal:** zero-mismatch enforcement. The ratchet semantic explicitly allows known-bad cases to coexist with new code — they tighten when fixed, not before.

## Verification checklist

- [ ] `cargo check` clean
- [ ] `cargo test --lib feedback_to_label` passes (existing + new schema test)
- [ ] `cargo test --bin labeled_regression_cli` (if it has unit tests) or manual verification against fixtures
- [ ] `cargo run --bin labeled_regression_cli -- --all --bootstrap-expected-failures` populates label files
- [ ] `cargo run --bin labeled_regression_cli -- --all --fail-on-regression` exits 0
- [ ] `cargo run --bin labeled_regression_cli -- 2026-04-30_$X.json --verbose` shows expected failures separately from regressions
- [ ] `./scripts/preflight.sh --regression` runs Layers 6+7+8+9 and exits 0
- [ ] `./scripts/preflight.sh` (default) unchanged
- [ ] `./scripts/preflight.sh --full` still runs all 9 layers
- [ ] CLAUDE.md and `tests/fixtures/labels/README.md` updated
- [ ] Runner setup doc written
