# Continuous Mode Test Harness — Design

**Status:** approved (brainstorming phase)
**Author:** zohoora
**Date:** 2026-04-18
**Scope:** testing infrastructure to enable safe decomposition of `run_continuous_mode` (currently a ~2,700-line async function)

---

## Problem

`tauri-app/src-tauri/src/continuous_mode.rs` is a 3,608-line file whose body is dominated by a single ~2,700-line async function, `run_continuous_mode`. Recent git history shows that conceptually-simple changes (e.g., the Apr 16 billing/merge bug fix) require touching 8–10 disjoint places inside this function. That "scatter tax" will grow with every new feature.

We want to decompose the function into focused sub-components (e.g., merge-back coordinator, detection tick handler, sleep-mode gate, screenshot loop, server-config reconciler). Before touching the body, we need a test harness that can prove behavior equivalence across the refactor.

The existing test infrastructure is unusually rich — `detection_replay_cli`, `merge_replay_cli`, `clinical_replay_cli`, `multi_patient_replay_cli`, `labeled_regression_cli`, `golden_day_cli`, `benchmark_runner`, the `e2e_*` integration suite, and `scripts/replay_day.py` — but **none of them actually drive `run_continuous_mode`**. Verification:

| Tool | What it invokes |
|------|-----------------|
| `detection_replay_cli` | `evaluate_detection()` — pure function |
| `merge_replay_cli`, `clinical_replay_cli`, `multi_patient_replay_cli`, `multi_patient_split_replay_cli` | `LLMClient::generate()` + task-specific parsers |
| `labeled_regression_cli`, `golden_day_cli` | Static disk comparison against hand-labeled corpus (no re-execution) |
| `e2e_layer5_continuous_mode_full` | Hand-rolled 6-step sequence; file comment at `e2e_tests.rs:794` explicitly says "exercises the same code paths as `run_continuous_mode()`, **minus the Tauri event emission, pipeline thread management, and periodic detection loop**" |
| `scripts/replay_day.py` | 714-line Python re-implementation that calls STT/LLM routers directly via HTTP |

Consequence: **any orchestrator-level regression introduced during decomposition would slip through the entire existing test suite**. The first signal would be wrong output in production (Room 2 / Room 6).

This spec defines a harness that fills the gap.

---

## Goals

1. **Drive the real `run_continuous_mode`** — not a parallel implementation — from archived production inputs.
2. **Deterministic.** No wall clock, no live LLM, no live STT, no network. Same input → same output, always.
3. **Fast enough for per-PR CI** at the per-encounter tier; on-demand execution acceptable at the per-day tier.
4. **First-divergence reporting.** When a refactor breaks behavior, the report points at the exact encounter, segment, and decision that first diverged from baseline — not a wall of cascading downstream differences.
5. **Per-test opt-in strictness** for LLM prompts and event sequences, so the harness is rigorous by default but not hostile to legitimate future work (prompt tuning, new events).

## Non-goals

- **Byte-identical output.** We do not compare timestamps, UUIDs, or wall-clock durations.
- **SOAP text equivalence.** SOAP content is LLM-generated and non-deterministic across temperatures; we compare presence/structure but not text.
- **Prompt accuracy validation.** That is the existing replay CLIs' job; this harness is for orchestration equivalence.
- **Frontend behavior testing.** Out of scope — vitest + `e2e_tests.rs` cover the frontend and the Tauri IPC surface.
- **Replacing the existing replay CLIs.** They cover a different failure mode (LLM prompt + parser accuracy); this harness complements them.
- **Load / performance / chaos testing.** Out of scope.

---

## Architecture

Three layers sitting on a single surgical refactor.

```
                           PRODUCTION CODE PATH                    TEST CODE PATH
                           ─────────────────────                   ──────────────

Layer 0                       run_continuous_mode( impl RunContext, Config, Handle, SyncCtx )
(untouched logic)                                     │
                                                      │  same function, both paths
                                                      │  reach it through the trait
                                                      ▼
                                            ┌────────────────────┐
                                            │   RunContext trait │   ←── the seam we extract
                                            └────────────────────┘
                                               ▲              ▲
                                     ┌─────────┘              └──────────┐
                                     │                                   │
Layer 1                    ┌────────────────────┐              ┌────────────────────┐
(Tauri seam)               │ TauriRunContext    │              │ RecordingRunContext│
                           │  - AppHandle wrap  │              │  - drives from     │
                           │  - real LLMClient  │              │    ReplayBundle    │
                           │  - real STT WS     │              │  - captures events │
                           │  - real sensor src │              │  - mock clock      │
                           │  - real clock      │              │  - mock LLM/STT    │
                           └────────────────────┘              └────────────────────┘
                                                                         ▲
Layer 2                                                        ┌─────────┴─────────┐
(harness driver)                                               │ EncounterHarness  │
                                                               │  DayHarness       │
                                                               │  - load bundle(s) │
                                                               │  - drive context  │
                                                               │  - collect output │
                                                               └─────────┬─────────┘
                                                                         │
Layer 3                                                        ┌─────────┴─────────┐
(comparator + report)                                          │ ArchiveComparator │
                                                               │ EventComparator   │
                                                               │ MismatchReport    │
                                                               │  - first-divergence│
                                                               │  - drill-in focus │
                                                               └───────────────────┘
```

### Data flow (per-encounter test)

1. Test loads `2026-04-14/abc12345/replay_bundle.json`. Bundle contains all captured inputs (segments, sensor transitions, vision results) and expected outputs (detection checks with LLM responses, decisions, outcomes, archived SOAP/billing refs).
2. Test constructs a `RecordingRunContext` seeded from the bundle: mock LLM keyed by `(task, prompt_hash) → recorded_response`, mock STT pre-loaded with captured segments, `MockSource` pre-loaded with captured sensor transitions, mock clock starting at the bundle's recorded encounter start time.
3. Test invokes the **real** `run_continuous_mode(ctx, ...)` under a tokio runtime. Same `select!`, same state transitions, same emissions as production. Every dependency request routes through `ctx`.
4. Driver loop advances virtual time in response to scheduled inputs. When the bundle's inputs are exhausted, it signals stop via `ctx.signal_stop()`.
5. Harness collects captured events, produced archive dirs (tempdir), LLM call log.
6. `ArchiveComparator` (and optionally `EventComparator`) diffs produced output against bundle baseline. First-divergence walk emits a structured `MismatchReport`.
7. Test assertion passes iff no mismatch.

---

## Section 1: The `RunContext` seam

The only refactor that must land before the harness can exist. Everything else is built on top.

### The trait

```rust
pub trait RunContext: Clone + Send + Sync + 'static {
    // Event emission
    fn emit(&self, event_name: &str, payload: impl Serialize);
    fn emit_to_window(&self, window: &str, event_name: &str, payload: impl Serialize);

    // Managed state access
    fn server_config(&self) -> Arc<ServerConfigSnapshot>;

    // Outbound dependencies (currently constructed inline from Config)
    fn llm_client(&self) -> Arc<dyn LlmBackend>;
    fn stt_backend(&self) -> Arc<dyn SttBackend>;
    fn sensor_source(&self) -> Box<dyn SensorSource>;

    // I/O
    fn archive_root(&self) -> &Path;

    // Clock (wrapping Utc::now / Local::now / tokio::time::sleep)
    fn now_utc(&self) -> DateTime<Utc>;
    fn now_local(&self) -> DateTime<Local>;
    fn sleep(&self, dur: Duration) -> impl Future<Output = ()> + Send;
    fn interval(&self, period: Duration) -> impl Stream<Item = ()> + Send;
}
```

Three new/reused backend traits:

- **`LlmBackend`** — new trait wrapping `LLMClient`. Methods: `generate`, `generate_timed`, `generate_vision`, `generate_vision_timed`. `impl LlmBackend for LLMClient` is a thin forwarding impl.
- **`SttBackend`** — new trait wrapping whatever the orchestrator currently awaits for segments. Exact seam to be confirmed during Phase 1 — likely the `mpsc::Receiver<Utterance>` that the pipeline task writes into.
- **`SensorSource`** — existing trait in `presence_sensor/sensor_source.rs`. No changes.

### Production impl: `TauriRunContext`

```rust
pub struct TauriRunContext {
    app: tauri::AppHandle,
    archive_root: PathBuf,
    llm: Arc<LLMClient>,
    stt: Arc<WhisperServerClient>,
}

impl RunContext for TauriRunContext {
    fn emit(&self, ev, p) { let _ = self.app.emit(ev, p); }
    fn server_config(&self) -> Arc<ServerConfigSnapshot> {
        let shared = self.app.state::<SharedServerConfig>();
        Arc::new(shared.blocking_read().snapshot())
    }
    fn now_utc(&self) -> DateTime<Utc> { Utc::now() }
    fn sleep(&self, d) -> impl Future { tokio::time::sleep(d) }
    // ...etc
}
```

Every call site of `app.emit(...)`, `Utc::now()`, `Local::now()`, `tokio::time::sleep(...)`, `shared.read()` inside `run_continuous_mode` becomes the corresponding `ctx.*` call. Mechanical refactor, no logic changes. Estimated 150–200 lines touched across `continuous_mode.rs`, `commands/continuous.rs`, plus forwarding impls in `llm_client.rs` and `whisper_server.rs`.

### Test impl: `RecordingRunContext`

```rust
pub struct RecordingRunContext {
    captured_events: Arc<Mutex<Vec<CapturedEvent>>>,
    server_config: Arc<ServerConfigSnapshot>,
    llm: Arc<ReplayLlmBackend>,
    stt: Arc<ScriptedSttBackend>,
    sensor: Arc<ScriptedSensorSource>,
    archive_root: PathBuf,        // tempdir per test
    // Clock: delegates to tokio::time (paused mode); no per-context state needed
}
```

Clock strategy: **tokio paused-time** (`#[tokio::test(flavor = "current_thread", start_paused = true)]`). `ctx.sleep(d)` forwards to `tokio::time::sleep(d)`; the driver calls `tokio::time::advance(d)` to move virtual time. Battle-tested, no custom clock infrastructure needed.

### Files touched by the refactor

- `src/continuous_mode.rs` — signature change, body edits (`app.emit` → `ctx.emit`, `Utc::now` → `ctx.now_utc`, etc.)
- `src/commands/continuous.rs` — construct `TauriRunContext` from `AppHandle`, pass to `run_continuous_mode`
- `src/llm_client.rs` — add `trait LlmBackend` + `impl LlmBackend for LLMClient`
- `src/whisper_server.rs` (or wherever the STT seam lives — to verify) — add `trait SttBackend` + impl for the production client
- `src/lib.rs` — expose new traits from crate root
- New: `src/run_context.rs` — the trait + `TauriRunContext` struct (keeps the seam visible)

### Rejected alternatives

- **Just extract `LlmBackend`**, skip clock/events/state. Can't replay time-dependent flows, can't capture events, can't inject config snapshots.
- **Separate traits per concern** (`EventBus`, `ConfigStore`, `Clock`, …). More surface area, more type parameters, more indirection than the single composed trait.
- **Pass dependencies as individual function args.** Signature would be 7+ params; every future dependency becomes a signature change.
- **Use `tauri::test::mock_builder()`** instead of the trait. Tauri's mock runtime has sharp edges (plugin state, event plugin timing, webview window lookup silently failing) and would couple the harness to Tauri binding internals.

---

## Section 2: Harness core

### Developer-facing API

```rust
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn encounter_2026_04_14_abc12345() {
    let bundle = ReplayBundle::load(
        "tests/fixtures/encounter_bundles/2026-04-14/abc12345.json"
    );

    EncounterHarness::new(bundle)
        .with_policy(EquivalencePolicy::ArchiveStructural)   // default
        .with_prompt_policy(PromptPolicy::Strict)            // default
        .run()
        .await
        .expect_equivalent();
}
```

`EncounterHarness` and `DayHarness` are the only types a test author touches. Policies:

```rust
enum EquivalencePolicy {
    ArchiveStructural,                     // default
    EventSequence,                         // ArchiveStructural + event ordering
}

enum PromptPolicy {
    Strict,                                // default: prompt-exact-match
    SequenceOnly { tasks: Vec<String> },   // per-test opt-out for listed task labels
}
```

### Mocks

**`ReplayLlmBackend`.** Constructed from `bundle.detection_checks`, `bundle.merge_check`, `bundle.clinical_check`, `bundle.multi_patient_detections`, `bundle.soap_result`, `bundle.vision_results`. Strict mode: builds `HashMap<(task_label, sha256(system_prompt ++ user_prompt)), response>`. On `generate(...)`, looks up the key. Miss → returns `Err(HarnessError::UnmatchedPrompt { task, prompt_hash, nearest_recorded })` that surfaces as a divergence in the mismatch report. SequenceOnly mode: `HashMap<task_label, VecDeque<response>>`, pops front per call.

**`ScriptedSttBackend`.** Real STT Router emits `Utterance` segments over a channel. The mock drives the same channel from `bundle.segments: Vec<ReplaySegment>`, honoring each segment's `relative_timestamp_secs` relative to encounter start. Virtual clock advance triggers segment delivery.

**`ScriptedSensorSource`.** Reuses existing `MockSource::new(...)` from `presence_sensor/sources/mock.rs`, seeded from `bundle.sensor_transitions`. Zero new code.

**Clock.** Covered in Section 1 — tokio paused-time.

### Driver loop

```
1. Construct RecordingRunContext from bundle (seeds all mocks).
2. Spawn run_continuous_mode(ctx.clone(), ...) on a tokio task.
3. Loop:
     next = min(next STT segment time, next sensor event time, next scheduled timer)
     tokio::time::advance(next - now)
     deliver scheduled inputs (STT segments, sensor events) whose time ≤ now
     yield_now in a bounded loop (max 50 iterations) until pending-inputs counter == 0
       AND all ctx-owned futures are pending (system quiesced)
4. When bundle inputs exhausted:
     ctx.signal_stop() → orchestrator's stop_flag → clean shutdown path
     await the spawned task (timeout guard: 5s virtual)
5. Collect ctx.captured_events, ctx.archive_root contents.
6. Hand off to Comparator.
```

The "yield until quiesced" step is the subtle correctness point. The bounded-iteration guard prevents infinite spin on a bug.

### Comparator

Two composable comparators:

**`ArchiveComparator`.** Walks expected archive state reconstructed from the bundle vs actual tempdir state. **Field-level with allowlist**: compares a known-stable set of metadata fields (`charting_mode`, `encounter_number`, `detection_method`, `patient_name`, `has_soap_note`, `has_billing_record`, `first_segment_index`, `last_segment_index`, presence of sibling `replay_bundle.merged_*.json` files). Explicitly ignores: timestamps, UUIDs, SOAP text content, raw audio. Allowlist is in code, version-controlled, reviewable.

**`EventComparator`** (opt-in via `EquivalencePolicy::EventSequence`). Compares `ctx.captured_events` against canonical expected sequence reconstructed from bundle. Compares `event_type` + key payload fields; ignores timestamps.

### First-divergence walk

```
1. Events (if EventSequence policy active):
     iterate captured vs expected in parallel
     first index where they disagree → report, stop
2. Archive:
     iterate sessions in encounter_number order
     for each session: walk metadata fields in allowlist order
     first mismatch → report, stop
3. Both pass → Equivalent. Otherwise → Divergent(report).
```

The first-divergence principle is the key UX property. Cascading downstream differences are suppressed; only the earliest disagreement + preceding-3-events context is reported.

### `MismatchReport` format

```jsonc
{
  "test_id": "encounter_2026_04_14_abc12345",
  "bundle_path": "tests/fixtures/encounter_bundles/2026-04-14/abc12345.json",
  "verdict": "Divergent",
  "first_divergence": {
    "kind": "DetectionDecision",
    "at_segment_index": 342,
    "at_event_index": 47,
    "expected": { "complete": true, "end_segment_index": 345, "confidence": 0.91 },
    "actual":   { "complete": false, "end_segment_index": null, "confidence": 0.62 },
    "preceding_events": [ /* last 3 */ ],
    "drill_in_command": "cargo test encounter_2026_04_14_abc12345 -- --nocapture --include-ignored --test-threads=1"
  },
  "summary_one_liner": "..."
}
```

The `summary_one_liner` prints to stderr on failure. Full JSON lands at `target/harness-report/<test_id>.json` (CI artifact). The drill-in command re-runs the test with verbose logging scoped to the divergence window (enabled via a `HARNESS_FOCUS` env var).

Failure `kind` values: `DetectionDecision`, `MergeDecision`, `MultiPatientSplit`, `ArchiveField`, `MissingArchiveFile`, `UnexpectedArchiveFile`, `EventPayload`, `MissingEvent`, `UnexpectedEvent`, `UnmatchedPrompt`, `OrchestatorPanic`, `OrchestratorTimeout`.

---

## Section 3: Per-day harness

Same pattern as per-encounter, scaled up.

**Input.** A date directory: `tests/fixtures/days/2026-04-14/` containing `day_log.jsonl` + one subdirectory per session, each with its own `replay_bundle.json` + `segments.jsonl` + `metadata.json`.

**Driver.** Load all bundles in the day, order by encounter start. Feed through a single orchestrator run. Virtual clock spans the whole recorded day (clock jumps over idle gaps instead of replaying them — we're testing orchestration, not idleness).

**Additional invariants** (beyond per-encounter's archive-structural check):

- `recent_encounters` list never contains session IDs that were merged away.
- Sleep-mode entry and exit events are paired (no enter without a matching exit in the same day).
- No orphan SOAP at end-of-day (every encounter with `has_soap_note=true` in baseline has it in actual).
- Every merge-back produces a sibling `replay_bundle.merged_*.json` under the surviving session's directory.
- Retrospective multi-patient split fires when warranted (baseline `multi_patient_detections.split_decision.decision_at_stage == Retrospective` is mirrored in actual).
- Day-log rotation: if the recorded day crosses midnight, both date subdirectories exist in actual output.

**Speed.** ~20-30 seconds per day with mocked LLM. Not per-PR CI; runs on-demand / nightly / before-merge / before-release.

**Count.** 6 labeled days already exist (`04-08`, `04-09`, `04-10`, `04-13`, `04-14`, `04-15` — per `docs/TESTING.md`). All become day fixtures after bootstrap.

---

## Section 4: Fixture management

### Seeding the corpus

Encounter bundles already exist in every archived session under `~/.transcriptionapp/archive/YYYY/MM/DD/{session_id}/replay_bundle.json`. A new bootstrap tool copies them into `tests/fixtures/encounter_bundles/`:

```bash
cargo run --bin bootstrap_harness_fixture -- --from-session 2026-04-14/abc12345
cargo run --bin bootstrap_harness_fixture -- --from-day 2026-04-14   # all encounters in a day
```

The tool copies the bundle, strips machine-identifying metadata if any, verifies it loads cleanly via `ReplayBundle::load()`, and runs it once through the harness to confirm pass-on-baseline before committing.

For the initial seed: 20–30 diverse encounter bundles covering:
- Simple single-patient encounter (no merge, no multi-patient, no sensor issues)
- Encounter with merge-back
- Multi-patient encounter with retrospective split
- Sensor-triggered split (hybrid mode)
- Sensor-continuity gate raising confidence threshold
- Non-clinical encounter (clinical check → skip SOAP)
- Force-split at word-cap thresholds
- Vision early-stop reached
- Sleep-mode boundary (encounter that spans sleep entry)
- Flush-on-stop path (>100 words buffered at stop)

### Updating baselines when intent changes

When a developer legitimately changes orchestrator behavior (e.g., adjusts a threshold), baselines need to update:

```bash
UPDATE_HARNESS_BASELINES=1 cargo test --test harness_per_encounter
```

Each test that now diverges overwrites its `expected_*` fields in the fixture with the new actual values, and the test then passes. The developer reviews the diff via `git diff tests/fixtures/encounter_bundles/` before committing. Standard snapshot-test workflow.

### Adding a new test case to an existing day

When Room 6 captures an encounter with an interesting new property (e.g., a new edge case worth locking in), the developer:

```bash
cargo run --bin bootstrap_harness_fixture -- \
    --from-session 2026-04-18/xyz98765 \
    --label "edge_case_consecutive_merge_backs"
```

The tool adds the bundle to the fixture directory with the given label and scaffolds a test function stub in `tests/harness_per_encounter.rs`.

---

## Section 5: Build sequence

Phased to keep each step small, reviewable, and independently testable. Each phase ships green.

| Phase | Work | Est. | Exit criteria |
|-------|------|------|---------------|
| 1 | Extract `LlmBackend` + `SttBackend` traits (forwarding impls on existing types). No behavior change. | 0.5 day | `cargo check` + `cargo test --lib` pass |
| 2 | Introduce `RunContext` trait + `TauriRunContext`. Change `run_continuous_mode` signature. Update `commands/continuous.rs` call site. Pure mechanical refactor. | 1 day | Preflight `--full` passes; Room 6 runs a half-day production session with no observable regression |
| 3 | `RecordingRunContext` + mocks (`ReplayLlmBackend`, `ScriptedSttBackend`, seeded `ScriptedSensorSource`) + driver loop. No comparator yet — smoke-test by confirming orchestrator runs to completion on 3 bundles without panicking. | 1–2 days | 3 hand-seeded bundles execute through the harness without error |
| 4 | `ArchiveComparator` + `MismatchReport` + first-divergence walk + stderr one-liner + JSON artifact. Wire `EncounterHarness` API. | 1 day | 3 seed bundles pass `expect_equivalent()` against their own baselines |
| 5 | `EventComparator` (opt-in policy). Deliberate-regression self-test: introduce a known bad change to `run_continuous_mode`, confirm harness catches it at the right divergence point. Revert. | 0.5 day | Deliberate-regression test fails with correct `kind` + `at_segment_index` |
| 6 | `DayHarness` — reuses per-encounter machinery, adds cross-encounter invariants. Bootstrap 3 day fixtures. | 1 day | All 3 day fixtures pass |
| 7 | Fixture management tooling: `bootstrap_harness_fixture` bin + `UPDATE_HARNESS_BASELINES=1` support. Expand corpus to 20–30 encounter bundles + all 6 days. | 0.5 day | 20+ encounter tests + 6 day tests, all passing, in CI |
| 8 | Wire into `scripts/preflight.sh` (per-encounter tier only; per-day stays on-demand). Update `docs/TESTING.md`. | 0.5 day | `preflight.sh --full` runs harness tier; docs reflect new layer |

**Total: ~6 focused days** before the larger `run_continuous_mode` decomposition begins.

Phases 1–2 are the only ones that touch production code paths; they're the only ones that need clinic-day production verification. Phases 3–8 are additive — they can't break the app, only the new test suite.

---

## Success criteria

The harness is considered "really good" (per the original user framing) when all the following hold:

1. **Corpus:** ≥20 encounter bundles spanning the diversity list in Section 4, plus ≥3 day fixtures.
2. **Green on main:** all fixtures pass on current `main` before any decomposition begins.
3. **Deliberate-regression self-test passes** — a known-bad change to `run_continuous_mode` produces a `MismatchReport` whose `first_divergence.kind` and `at_segment_index` point at the right place on first read.
4. **Speed:** per-encounter tier completes in <60s on the Room 6 development machine (CI budget).
5. **Wired in:** per-encounter tier runs as a layer in `scripts/preflight.sh --full`; per-day tier is documented as an on-demand gate in `docs/TESTING.md`.
6. **Failure UX:** a simulated failure is diagnosable-to-root-cause in <10 minutes from the stderr one-liner alone, without opening the JSON artifact.

When all six hold, the decomposition of `run_continuous_mode`'s body proceeds in its own separate spec, with the harness as its safety net.

---

## Open questions

- **Exact STT seam to wrap in `SttBackend`.** The file map suggests the pipeline task writes utterances into an `mpsc::Receiver` that `run_continuous_mode` awaits. To confirm by inspection during Phase 1; if it turns out the orchestrator holds the receiver rather than reading from a client directly, the trait wraps "thing that hands out a receiver" rather than the client itself.
- **Whether `ServerConfigSnapshot` needs to be a new struct** or whether we can re-expose the existing cached-config types. Leaning toward reusing what's there; will resolve in Phase 2.
- **How to handle vision LLM calls in strict mode.** Vision prompts include screenshot bytes; hashing those as part of `(task, prompt_hash)` is expensive. Likely answer: strip the image payload from the hash input and hash only the text-prompt portion, since vision responses are keyed to on-screen patient name/DOB content which the bundle already captured. Resolve in Phase 3.

---

## Related documents

- `docs/TESTING.md` — authoritative test architecture (7 layers today; this adds an 8th)
- `tauri-app/CLAUDE.md` — codebase patterns and gotchas
- `docs/superpowers/specs/2026-04-13-mobile-app-v1-design.md` — same design-doc format, recent precedent
- `docs/adr/0023-server-configurable-data.md` — relevant to `ServerConfigSnapshot` shape
