# ADR-0028: Replay Logging Architecture

## Status

Accepted (Apr 2026)

## Context

Production LLM decisions are non-deterministic — the same prompt at the same temperature can flip a split/no-split call roughly 40% of the time. Traditional unit tests with mocked LLM responses don't catch prompt regressions, model drift, or threshold calibration issues.

We need offline regression testing against **real production data**: capture every input a real session gave the LLM, then replay those inputs against a new model/prompt/threshold to measure behavior change.

Constraints:

1. **Self-contained bundles** — a replay test case must include everything needed to reproduce a decision (segments, sensor state, timestamps, config snapshot).
2. **Schema-versioned** — the capture format evolves; old bundles must still load or be upgradable.
3. **Backward-compatible deserialization** — a new field added for v3 must not break loading v1/v2 bundles.
4. **Zero overhead on hot paths** — production continuous mode can't afford blocking writes during encounter detection.
5. **Per-encounter scoped** — a new bundle starts when an encounter begins and ends when it splits or merges.

## Decision

Three complementary logging tiers, each written with `#[serde(default)]` on every field for forward-compat.

### Tier 1: `segment_log` — per-segment JSONL

`SegmentLogger` writes one JSON line per transcript segment as it arrives. This is the lowest-level timeline — every STT utterance is captured with its timestamp, speaker, confidence, word count, and sensor state at the moment of ingestion.

**Buffering trick**: when continuous mode starts, the session directory doesn't exist yet (it's created at first archive). The logger buffers the first ~N segments in memory and flushes to disk once the directory is known. The file handle is held open for the rest of the session — amortizes open/close cost.

Use case: detailed timeline debugging, correlation with sensor CSV logs.

### Tier 2: `replay_bundle` — per-encounter self-contained

`ReplayBundleBuilder` is an **accumulator** that collects every input to every LLM decision in the encounter, plus the config snapshot, plus the final outcome. When the encounter splits or merges, `build_and_reset()` serializes to `replay_bundle.json` in the session directory and clears the accumulator for the next encounter.

```rust
pub struct ReplayBundle {
    pub schema_version: u32,         // currently 3
    pub config: serde_json::Value,    // snapshot of all relevant config
    pub segments: Vec<ReplaySegment>,
    pub sensor_transitions: Vec<SensorTransition>,
    pub vision_results: Vec<VisionResult>,
    pub loop_state: LoopState,        // v2+: sensor_continuous_present, etc.
    pub detection_checks: Vec<DetectionCheck>,
    pub clinical_check: Option<ClinicalCheck>,
    pub multi_patient_detections: Vec<MultiPatientDetection>, // v2+
    pub split_decision: Option<SplitDecision>,
    pub merge_check: Option<MergeCheck>,
    pub outcome: Option<Outcome>,
}
```

**Merge-back semantics**: when encounter A merges into encounter B, `build_merged_and_reset(session_dir, surviving_id)` writes the bundle as a sibling file in B's directory (named `replay_bundle_merged_A.json`) so the merged-away encounter's decision trace isn't lost.

**Critical invariant**: the accumulator state must not leak across encounters. `test_no_state_leaks_after_clear_then_build` pins this — encounter A's merged bundle must clear all data before encounter B accumulates.

**Zero-copy reset**: `build_and_reset()` uses `std::mem::take()` to transfer the data into a temporary `ReplayBundle` for serialization, then resets the builder's fields via `Default::default()`. Avoids a clone of potentially-large `segments` vecs.

### Tier 3: `day_log` — day-level JSONL

`DayLogger` writes one JSON line per encounter-level event (encounter started, detection check, split decision, merge decision, SOAP generated) across the whole day. Unlike bundles (scoped per encounter), day_log lets the `golden_day_cli` regression tool replay an entire clinic day in order.

Rotates on local-midnight boundary.

### Schema versioning

`SCHEMA_VERSION: u32` constant in `replay_bundle.rs` is the single source of truth. Current value: **3**.

| Version | Added |
|---------|-------|
| v1 | Baseline — segments, sensor transitions, vision, detection checks, clinical check, outcome |
| v2 | `sensor_continuous_present`, `sensor_triggered`, `manual_triggered` on `LoopState`; `multi_patient_detections` field; merge-back sibling files |
| v3 | `MultiPatientDetection.split_decision` — captures the multi-patient SPLIT prompt's parsed `line_index` for replay regression testing |

New fields all use `#[serde(default)]` — older bundles load fine, older replay tools see `None`.

**Drift guard**: `test_schema_version_docs_in_sync` in `replay_bundle.rs` asserts that `tauri-app/CLAUDE.md` and `docs/benchmarks/multi-patient-split.md` contain the literal current `SCHEMA_VERSION` value. Bumping the constant without updating those docs fails the test with a clear error.

**Historical upgrades**: the `replay_bundle_backfill` CLI reconstructs v1→v2 upgrades from mmWave CSV + day_log for historical data. v2→v3 upgrade happens organically as new bundles are written; old v2 bundles remain loadable.

### Consumer tools

12 CLIs in `tools/` consume the bundles for offline regression:

| Tool | What it replays |
|------|-----------------|
| `detection_replay_cli` | Rerun `evaluate_detection()` over captured inputs, compare decisions |
| `merge_replay_cli` | Re-issue the merge-check LLM call with archived inputs |
| `clinical_replay_cli` | Re-issue clinical content check |
| `multi_patient_replay_cli` | Re-issue multi-patient detection |
| `multi_patient_split_replay_cli` | Re-issue split-point detection |
| `benchmark_runner` | Curated `tests/fixtures/benchmarks/*.json` |
| `labeled_regression_cli` | Production billing vs labeled ground truth |
| `golden_day_cli` | Full clinic-day regression |
| `bootstrap_labels` | Generate label fixtures from production output |
| `replay_bundle_backfill` | v1 → v2 historical upgrade |
| `encounter_experiment_cli` | A/B test prompt variants on archived sessions |
| `vision_experiment_cli` | A/B test vision strategies |

See `docs/TESTING.md` for the authoritative test infrastructure overview.

## Consequences

### Positive

- **Offline regression tests against real data** — catch prompt drift, model changes, threshold miscalibration without running a live clinic.
- **Schema evolution without breaking old data** — `#[serde(default)]` + drift test keeps historical bundles replayable.
- **Zero-copy builder reset** — negligible overhead on the hot encounter path.
- **Per-tier granularity** — segments for timeline, bundles for decisions, day_log for day-level orchestration.

### Negative

- **Non-trivial serialization surface** — many nested structs with `#[serde(default)]` annotations; easy to forget one on a new field. A lint-style test could enforce this.
- **Bundle size grows with encounter length** — a 5,000-word encounter with 20 detection checks and full segment capture can produce a 200KB bundle. Acceptable for offline regression use; not free at scale.
- **Merge-back complexity** — the sibling-file pattern (`replay_bundle_merged_A.json` in B's directory) is surprising on first read. Documented in the `build_merged_and_reset` docstring but still a gotcha for new contributors.

## References

- `tauri-app/src-tauri/src/replay_bundle.rs` — accumulator + schema version
- `tauri-app/src-tauri/src/segment_log.rs` — segment timeline
- `tauri-app/src-tauri/src/day_log.rs` — day-level orchestration
- `tauri-app/src-tauri/src/pipeline_log.rs` — pipeline message trace
- `docs/TESTING.md` — 7-layer test architecture referencing these tiers
- ADR-0024: Hybrid Encounter Detection — primary producer of bundles
- ADR-0027: Retrospective Multi-Patient Check — adds v3 `split_decision` capture
