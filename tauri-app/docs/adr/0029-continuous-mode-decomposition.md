# ADR-0029: Continuous-Mode Detector Decomposition

## Status

Accepted (Apr 2026, v0.10.41–v0.10.44)

## Context

`continuous_mode.rs` grew to 3,610 lines with a single 2,700-line `run_continuous_mode` async function. Two recent production bugfixes (`cd2acc3` small-orphan auto-merge, `d77d691` retrospective multi-patient handling) each required 10+ scattered hunks across the same region — the "scatter tax" of a single large function whose phases are woven together instead of separated.

The per-encounter test harness (`tests/harness_per_encounter.rs`, landed Apr 2026) now captures enough of the detector loop's behavior to validate a refactor — every LLM call, event, and archive mutation becomes a snapshot assertion. Without that harness, decomposing a 2,700-line function while preserving behavior would have required full E2E clinic-day testing per extraction; with it, each extraction is a one-commit, one-test-run operation.

Constraints on the decomposition:
1. **No behavior change.** The harness (10 seed bundles) must stay green across every extraction commit.
2. **No new abstractions.** Each extraction is a *move*, not a rewrite — same logic, same call order, same events emitted.
3. **Debuggability at 9am next clinic day.** Every extracted module's `info!`/`warn!` gets `event = "..."` + `component = "..."` structured fields so production logs can be filtered per-module.
4. **One commit per extraction**, in an order that starts with the most isolated region to establish the pattern before touching the detector task's mutable state.

## Decision

Extract seven peer modules at `src/continuous_mode_*.rs`, each owning a phase of the detector loop. The shell retains orchestration (spawning tasks, the outer trigger loop), the peers own phase logic.

### Peer modules

| Module | Purpose | Lines |
|---|---|---|
| `continuous_mode_trigger_wait.rs` | `select!` over timer/silence/manual/sensor + sensor state machine (absence tracking, continuous-presence flag, channel downgrade) | 314 |
| `continuous_mode_splitter.rs` | Buffer drain → archive → metadata enrichment → `EncounterDetected` event | 406 |
| `continuous_mode_post_split.rs` | Clinical-content check → pre-SOAP multi-patient detect → SOAP generation → billing extraction | 447 |
| `continuous_mode_merge_back.rs` | Four post-split safety nets: small-orphan auto-merge, LLM merge check, retrospective multi-patient split, standalone multi-patient check | 849 |
| `continuous_mode_forward_merge.rs` | Clean up a previous encounter's false multi-patient split when the next encounter is clearly the same patient as one of the sub-SOAPs (A/P-term overlap + audio-contiguity rule) | 538 |
| `continuous_mode_flush_on_stop.rs` | Shutdown pipeline: task cleanup, orphan SOAP/billing recovery, flush-remaining-buffer | 667 |
| `continuous_mode_events.rs` | Typed `ContinuousModeEvent` enum with `#[serde(tag = "type")]` | 348 |
| `continuous_mode_types.rs` | Shared `LoopState` struct (encounter_number + merge_back_count) | 40 |

### Shape pattern — each module follows the same template:

```rust
pub struct XDeps {
    // Long-lived Arc clones: logger, bundle, sync_ctx, llm_client, templates,
    // billing_data, day_logger, plus fields from DetectionThresholds.
}

pub struct XCall<'a> {
    // Per-invocation borrows: session_id, encounter_text, prev-encounter refs.
}

pub enum XOutcome {
    // Variants describing what the caller should do next.
}

pub async fn run<C: RunContext>(ctx: &C, deps: &XDeps, call: XCall<'_>, state: &mut LoopState) -> XOutcome
```

**`Deps` + `Call` split** — `Deps` is built once before the detector loop starts and borrowed by reference each iteration (no per-encounter Arc cloning). `Call` carries per-iteration borrowed refs (session_id, encounter text, prev-encounter handles). This shape keeps the hot path allocation-free while making test wiring trivial.

**`LoopState` threading** — two counters (`encounter_number`, `merge_back_count`) are mutated across multiple phases. Passed via `&mut` to the modules that need it. Every exit path writes every touched field (code-review discipline rather than a type-system guarantee).

**Logger redirect contract** — the pipeline logger is pointed at the current encounter's `pipeline_log.jsonl`. `splitter` redirects on split; `merge_back` redirects to the surviving session on LLM-merge and retrospective paths; `post_split` and `forward_merge` do NOT redirect. Each module's file-level comment documents its `LOGGER SESSION CONTRACT` so future maintainers don't break it.

### Extraction order

1. `flush_on_stop` — most isolated (shutdown only), de-risks the `Deps`/`Outcome` pattern
2. `splitter` — archives the encounter, produces a `SplitContext` consumed downstream
3. `post_split` — consumes `SplitContext`, generates SOAP, returns `PostSplitOutcome`
4. `merge_back` — biggest win (849 lines), consolidates four disjoint safety nets; directly eliminates the scatter-tax
5. `forward_merge` — new behavior (not a move), added as a peer module to mirror the existing shape
6. `trigger_wait` — most state-heavy; deferred to last so the shape pattern was fully established

### What stays in `continuous_mode.rs`

- Top-level types: `ContinuousModeHandle`, `ContinuousState`, `RecentEncounter`, `ContinuousModeStats`
- Helpers: `effective_soap_detail_level`, `tail_words`, `head_words`, `finalize_merged_bundle`, `multi_patient_from_outcome`
- Constants: `MIN_SPLIT_WORD_FLOOR`, `MIN_SENSOR_HYBRID_WORDS`, `MERGE_EXCERPT_WORDS`
- `run_continuous_mode` shell: spawns consumer + monitor + detector + screenshot tasks, runs the outer trigger loop, calls the peer modules in sequence
- Test module

## Consequences

**Size reduction:**
- `continuous_mode.rs`: 3,610 → 2,345 lines (-35%)
- `run_continuous_mode` function: 2,700 → 1,454 lines (-46%)

**Scatter-tax elimination:** a bug fix of the class that motivated decomposition (the 10-hunk scatter seen in `cd2acc3`/`d77d691`) now lands in one file. Verified on v0.10.43's forward-merge addition, which touched 3 files and added 1 new module instead of scattering across the detector task.

**Test impact:** 1,170 lib tests + 10 harness tests green across every extraction commit. Zero regressions introduced by decomposition itself; every bug surfaced during the work (vision JSON corruption, over-splitting, mislabel from early-stop) was pre-existing.

**Structured tracing:** every `info!` / `warn!` in the new modules carries `event = "..." component = "..."` fields matching `activity_log.rs` convention. Production debugging at 9am next clinic day is a jq filter: `jq 'select(.component=="continuous_mode_merge_back")'`.

**What we didn't do** (considered, rejected):
- **Per-encounter task spawning.** Each module *could* have been a separate tokio task communicating via channels. Rejected: the detector loop's sequential nature (trigger → detect → split → post_split → merge_back) maps naturally to sequential function calls; tasks + channels would add debugging difficulty without concurrency benefit.
- **Trait-based abstraction.** An `EncounterPhase` trait could have unified the `run` signatures. Rejected: each phase has different inputs/outputs and state mutations; a trait would require trait objects with dynamic dispatch or a giant enum, neither of which improved over concrete functions.
- **Rename `ctx` param to `run_ctx`.** Originally planned to avoid collision with `let mut ctx = serde_json::json!(...)` pipeline-log patterns. Inspected post-extraction — no collisions exist in the decomposed modules (json-context usage only survives in `encounter_pipeline.rs`, which doesn't take a `ctx: C` parameter).
- **`ContinuousModeEvent::Error.component` field.** Planned addition. Inspected — only 2 emit sites, both in `continuous_mode.rs` where the component is unambiguous. Deferred until a use case surfaces.

## References

- Harness: `tauri-app/src-tauri/tests/harness_per_encounter.rs` (10 seed bundles)
- Per-module `LOGGER SESSION CONTRACT` doc-comments at the top of each extracted file
- Forward-merge rule (v0.10.43): `docs/adr/0029-continuous-mode-decomposition.md` discusses, details in `continuous_mode_forward_merge.rs` module doc-comment
- Pre-decomposition motivation commits: `cd2acc3` (small-orphan auto-merge, 10-hunk scatter), `d77d691` (retrospective multi-patient handling, similar pattern)
