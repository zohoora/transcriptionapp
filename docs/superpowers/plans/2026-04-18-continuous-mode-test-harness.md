# Continuous Mode Test Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an offline, deterministic test harness that drives the real `run_continuous_mode` function through archived production inputs, so the upcoming decomposition of its 2,700-line body is safe to perform.

**Architecture:** Extract a `RunContext` trait that abstracts the Tauri-specific surface (event emission, managed state, clock, LLM/STT/sensor backends) of `run_continuous_mode`. Production implements it via `TauriRunContext`; tests implement it via `RecordingRunContext` seeded from `replay_bundle.json`. A harness driver advances tokio's paused virtual time in lockstep with recorded inputs, captures outputs, and diffs against the recorded baseline using a first-divergence walk.

**Tech Stack:** Rust, Tokio (paused-time testing), Tauri v2, `sha2` for prompt hashing, `tempfile` for per-test archives, `serde_json` for reports.

**Related docs:**
- Spec: `docs/superpowers/specs/2026-04-18-continuous-mode-test-harness-design.md`
- Test architecture: `docs/TESTING.md`
- Project patterns: `tauri-app/CLAUDE.md`

**Workstation context:** Commands below assume CWD is `/Users/backoffice/transcriptionapp/tauri-app/src-tauri/` unless otherwise stated.

**Total estimate:** ~6–7 focused days across 8 phases.

**Phase verification vs clinic use:** Phase 2 is the only one that changes production code paths. After Phase 2 lands, run a half-day production session on Room 6 before continuing — this is cheap insurance against a missed `ctx.*` substitution. Phases 3–8 are additive; they add new files and tests but do not affect shipped behavior.

---

## File Structure

### New files (29)

**Trait seam:**
- `src-tauri/src/run_context.rs` — `RunContext` trait + `TauriRunContext` impl
- `src-tauri/src/llm_backend.rs` — `LlmBackend` trait + `impl LlmBackend for LLMClient`
- `src-tauri/src/stt_backend.rs` — `SttBackend` trait + impl

**Harness core (new module):**
- `src-tauri/src/harness/mod.rs`
- `src-tauri/src/harness/recording_run_context.rs`
- `src-tauri/src/harness/replay_llm_backend.rs`
- `src-tauri/src/harness/scripted_stt_backend.rs`
- `src-tauri/src/harness/scripted_sensor_source.rs`
- `src-tauri/src/harness/driver.rs`
- `src-tauri/src/harness/encounter_harness.rs`
- `src-tauri/src/harness/day_harness.rs`
- `src-tauri/src/harness/archive_comparator.rs`
- `src-tauri/src/harness/event_comparator.rs`
- `src-tauri/src/harness/mismatch_report.rs`
- `src-tauri/src/harness/policies.rs`
- `src-tauri/src/harness/captured_event.rs`

**Bootstrap CLI:**
- `src-tauri/tools/bootstrap_harness_fixture.rs`

**Integration tests:**
- `src-tauri/tests/harness_per_encounter.rs`
- `src-tauri/tests/harness_per_day.rs`

**Test fixtures (seeded in phases 3 + 7):**
- `src-tauri/tests/fixtures/encounter_bundles/` (20+ bundles)
- `src-tauri/tests/fixtures/days/` (6 day directories)

### Modified files (7)

- `src-tauri/src/continuous_mode.rs` — signature change + ~74 call-site substitutions (`app.emit` / `Utc::now` / `tokio::time::sleep` / etc. → `ctx.*`)
- `src-tauri/src/commands/continuous.rs` — construct `TauriRunContext`, pass to `run_continuous_mode`
- `src-tauri/src/llm_client.rs` — add `impl LlmBackend for LLMClient` (one block, forwarding)
- `src-tauri/src/whisper_server.rs` (or `pipeline.rs` — confirmed during Task 1.2) — add `impl SttBackend` for production STT type
- `src-tauri/src/lib.rs` — pub-use new modules
- `src-tauri/Cargo.toml` — register `bootstrap_harness_fixture` as `[[bin]]`
- `tauri-app/scripts/preflight.sh` — add harness tier (Phase 8)
- `docs/TESTING.md` — document 8th test layer (Phase 8)

---

## Phase 1: Backend traits (0.5 day)

Extract pure forwarding traits for the two outbound dependencies that must become mockable: LLM and STT. No behavior change. Compiler-verified.

### Task 1.1: Create `LlmBackend` trait

**Files:**
- Create: `src-tauri/src/llm_backend.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod llm_backend;`)
- Modify: `src-tauri/src/llm_client.rs` (add impl block at end)

- [ ] **Step 1: Write the trait file**

Create `src-tauri/src/llm_backend.rs`:

```rust
//! Trait wrapping LLMClient so run_continuous_mode can be driven with a mock in tests.
//!
//! The production impl is a thin forwarding wrapper — no behavior change from
//! calling LLMClient directly. The test impl (ReplayLlmBackend in src/harness/)
//! returns recorded responses keyed by prompt hash.

use crate::llm_client::{CallMetrics, LLMClient};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait LlmBackend: Send + Sync + 'static {
    async fn generate(
        &self,
        model: &str,
        system: &str,
        user: &str,
        task_label: &str,
    ) -> Result<String, String>;

    async fn generate_timed(
        &self,
        model: &str,
        system: &str,
        user: &str,
        task_label: &str,
    ) -> (Result<String, String>, CallMetrics);

    async fn generate_vision(
        &self,
        model: &str,
        system: &str,
        user: &str,
        image_bytes: Vec<u8>,
        task_label: &str,
    ) -> Result<String, String>;

    async fn generate_vision_timed(
        &self,
        model: &str,
        system: &str,
        user: &str,
        image_bytes: Vec<u8>,
        task_label: &str,
    ) -> (Result<String, String>, CallMetrics);
}

#[async_trait]
impl LlmBackend for LLMClient {
    async fn generate(&self, model: &str, system: &str, user: &str, task: &str) -> Result<String, String> {
        LLMClient::generate(self, model, system, user, task).await
    }

    async fn generate_timed(&self, model: &str, system: &str, user: &str, task: &str) -> (Result<String, String>, CallMetrics) {
        LLMClient::generate_timed(self, model, system, user, task).await
    }

    async fn generate_vision(&self, model: &str, system: &str, user: &str, image: Vec<u8>, task: &str) -> Result<String, String> {
        LLMClient::generate_vision(self, model, system, user, image, task).await
    }

    async fn generate_vision_timed(&self, model: &str, system: &str, user: &str, image: Vec<u8>, task: &str) -> (Result<String, String>, CallMetrics) {
        LLMClient::generate_vision_timed(self, model, system, user, image, task).await
    }
}

// Blanket impl so Arc<LLMClient> is also an LlmBackend
#[async_trait]
impl<T: LlmBackend + ?Sized> LlmBackend for Arc<T> {
    async fn generate(&self, m: &str, s: &str, u: &str, t: &str) -> Result<String, String> {
        (**self).generate(m, s, u, t).await
    }
    async fn generate_timed(&self, m: &str, s: &str, u: &str, t: &str) -> (Result<String, String>, CallMetrics) {
        (**self).generate_timed(m, s, u, t).await
    }
    async fn generate_vision(&self, m: &str, s: &str, u: &str, i: Vec<u8>, t: &str) -> Result<String, String> {
        (**self).generate_vision(m, s, u, i, t).await
    }
    async fn generate_vision_timed(&self, m: &str, s: &str, u: &str, i: Vec<u8>, t: &str) -> (Result<String, String>, CallMetrics) {
        (**self).generate_vision_timed(m, s, u, i, t).await
    }
}
```

**Note on `async_trait`:** Already in Cargo.toml (needed for tauri plugins). Verify with `grep '^async-trait' Cargo.toml` before starting; add if missing.

**Note on exact method signatures:** Before writing this task, inspect `src-tauri/src/llm_client.rs` around lines 469+ to confirm the exact signatures of `LLMClient::generate` et al. match above. The generate/generate_timed signatures are stable (documented in CLAUDE.md v0.10.36). If a method takes additional args not shown (e.g., a `reasoning` flag), add them verbatim — this is a pure forwarding impl, no interpretation.

- [ ] **Step 2: Wire module into crate**

Edit `src-tauri/src/lib.rs`: add `pub mod llm_backend;` alongside the other `pub mod` declarations (near the top of the file, alphabetically adjacent to `llm_client`).

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --lib`
Expected: compiles cleanly. Warnings about unused trait are fine — the trait is used in later phases.

- [ ] **Step 4: Write a smoke test**

Append to `src-tauri/src/llm_backend.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check that LLMClient implements LlmBackend without specialization surprises.
    fn _assert_llm_client_is_backend() {
        fn takes_backend(_: impl LlmBackend) {}
        // This function never runs — it just forces the compiler to check the impl.
        let _ = |c: LLMClient| takes_backend(c);
    }

    // Same for Arc<LLMClient>
    fn _assert_arc_llm_client_is_backend() {
        fn takes_backend(_: impl LlmBackend) {}
        let _ = |c: Arc<LLMClient>| takes_backend(c);
    }
}
```

This is a compile-only test — the assertions happen during type-checking. No runtime behavior to verify.

- [ ] **Step 5: Run the test**

Run: `cargo test --lib llm_backend`
Expected: 0 tests actually run (the `_assert_*` functions are compile-time only) but compilation passes.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/llm_backend.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
refactor(harness): extract LlmBackend trait for mockable LLM calls

Pure forwarding wrapper over LLMClient. Enables ReplayLlmBackend in the
upcoming orchestrator test harness to stand in for LLMClient without
touching any call site except the LLMClient type parameter.

No behavior change.

Part of the continuous-mode test harness — see
docs/superpowers/specs/2026-04-18-continuous-mode-test-harness-design.md

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 1.2: Identify the STT seam

Before writing `SttBackend`, we need to confirm what `run_continuous_mode` actually awaits for STT output. From the file map, the pipeline task writes into an `mpsc::Receiver<Utterance>`-style channel; the question is where the boundary lives.

**Files:**
- Read: `src-tauri/src/continuous_mode.rs` (find `utterance_rx` / `segment_rx` / `stt_rx` and its init site)
- Read: `src-tauri/src/whisper_server.rs` (public surface)
- Read: `src-tauri/src/pipeline.rs` (search for the pipeline → orchestrator channel)

- [ ] **Step 1: Grep for the receiver**

```bash
grep -n "Receiver\|utterance_rx\|segment_rx\|transcript.*rx\|Utterance" src-tauri/src/continuous_mode.rs | head -20
grep -n "Sender\|send.*Utterance" src-tauri/src/pipeline.rs | head -10
```

- [ ] **Step 2: Trace what the orchestrator actually awaits**

Find the `select!` branches in `run_continuous_mode` (around line 1089 based on earlier scan). Note the channel's message type and where it's constructed.

- [ ] **Step 3: Decide the trait shape**

Two patterns are likely:
- **(A) The orchestrator owns the receiver and awaits it directly.** Trait wraps the "channel factory": `fn subscribe(&self) -> Receiver<Utterance>` or similar.
- **(B) The orchestrator calls a method on an STT client that internally drives a channel.** Trait wraps the client.

Record the decision in a comment at the top of `src-tauri/src/stt_backend.rs` (next task) so future readers see the reasoning.

No code change in this task — it's a read-and-decide task. Its output is understanding, captured in the next task's file header.

- [ ] **Step 4: (No commit — this is investigation)**

---

### Task 1.3: Create `SttBackend` trait

**Files:**
- Create: `src-tauri/src/stt_backend.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod stt_backend;`)
- Modify: `src-tauri/src/whisper_server.rs` OR `src-tauri/src/pipeline.rs` (add impl block — target file depends on Task 1.2 outcome)

- [ ] **Step 1: Write the trait file**

Create `src-tauri/src/stt_backend.rs`:

```rust
//! Trait wrapping the STT seam so the orchestrator can be driven from a
//! pre-loaded segment stream in tests.
//!
//! Decision recorded during Task 1.2:
//!
//!   - Pattern A (orchestrator owns an `mpsc::Receiver<Utterance>` directly):
//!     trait wraps "handing out a receiver" — `subscribe() -> Receiver<Utterance>`.
//!   - Pattern B (orchestrator calls a client method that internally drives
//!     a channel): trait wraps the method; the channel is internal.
//!
//! The code below assumes Pattern A. If Task 1.2 revealed Pattern B, rewrite
//! the trait method to match the orchestrator's actual awaited call and note
//! the reason here.
//!
//! Production impl forwards to the existing STT subscription surface;
//! test impl (ScriptedSttBackend) drives the same channel from bundle.segments.

use async_trait::async_trait;
use tokio::sync::mpsc;
// Replace `Utterance` with the actual type discovered in Task 1.2:
use crate::transcription::Utterance;

#[async_trait]
pub trait SttBackend: Send + Sync + 'static {
    /// Subscribe to the STT segment stream.
    ///
    /// In production, returns a channel fed by the pipeline task.
    /// In tests, returns a channel pre-loaded with recorded segments.
    fn subscribe(&self) -> mpsc::Receiver<Utterance>;
}
```

**If Task 1.2 revealed pattern B (method-based rather than channel-based):** adjust the trait to expose the same method(s) the orchestrator calls. Whatever the trait shape is, it must exactly mirror what `run_continuous_mode` awaits on today.

- [ ] **Step 2: Write the production impl**

In the file determined by Task 1.2 (likely `src-tauri/src/pipeline.rs`), add:

```rust
use crate::stt_backend::SttBackend;
use async_trait::async_trait;

#[async_trait]
impl SttBackend for PipelineHandle {  // or WhisperServerClient, or whichever type holds the subscription
    fn subscribe(&self) -> tokio::sync::mpsc::Receiver<crate::transcription::Utterance> {
        // Forward to the existing subscription mechanism
        self.subscribe_utterances()  // replace with the actual method
    }
}
```

- [ ] **Step 3: Wire module into crate**

Edit `src-tauri/src/lib.rs`: add `pub mod stt_backend;`.

- [ ] **Step 4: Verify compilation**

Run: `cargo check --lib`
Expected: compiles cleanly.

- [ ] **Step 5: Compile-time impl check**

Append to `src-tauri/src/stt_backend.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_production_type_is_backend() {
        fn takes_backend(_: impl SttBackend) {}
        // Replace ProductionType with the actual type from Task 1.2:
        // let _ = |c: PipelineHandle| takes_backend(c);
    }
}
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/stt_backend.rs src-tauri/src/lib.rs src-tauri/src/pipeline.rs  # or whichever production file was modified
git commit -m "$(cat <<'EOF'
refactor(harness): extract SttBackend trait for mockable STT subscription

Pure forwarding wrapper. Enables ScriptedSttBackend in the upcoming
orchestrator test harness to replay recorded segments from a replay_bundle
in place of live STT Router streaming.

No behavior change.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 1.4: Phase 1 verification

- [ ] **Step 1: Full lib build**

Run: `cargo check --lib`
Expected: clean (no warnings about unused imports in the new files; if any, add `#[allow(unused)]` only as a last resort).

- [ ] **Step 2: Full lib test**

Run: `cargo test --lib`
Expected: 1,125 tests pass (current baseline per CLAUDE.md). Neither trait's compile-time test should break anything.

- [ ] **Step 3: (No commit — verification only. Phase 1 is done.)**

---

## Phase 2: `RunContext` trait + signature refactor (1–1.5 days)

The biggest phase by line count but purely mechanical. Every change is a find-and-replace inside `run_continuous_mode`.

### Task 2.1: Create `run_context.rs` with the trait skeleton

**Files:**
- Create: `src-tauri/src/run_context.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write the trait + stub TauriRunContext**

Create `src-tauri/src/run_context.rs`:

```rust
//! The runtime environment that run_continuous_mode runs inside.
//!
//! Production uses TauriRunContext (wraps AppHandle + real LLM/STT clients).
//! Tests use RecordingRunContext (see src/harness/) which drives from a
//! replay_bundle and captures emitted events.
//!
//! This seam is the narrowest possible abstraction that makes run_continuous_mode
//! testable offline. No other orchestrator code needs to change once this trait
//! is in place.

use crate::commands::physicians::SharedServerConfig;
use crate::continuous_mode_events::ContinuousModeEvent;
use crate::llm_backend::LlmBackend;
use crate::server_config::ServerConfigSnapshot;
use crate::stt_backend::SttBackend;
use chrono::{DateTime, Local, Utc};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;

/// The runtime surface run_continuous_mode depends on.
pub trait RunContext: Clone + Send + Sync + 'static {
    // --- Event emission ---

    fn emit_continuous_event(&self, event: &ContinuousModeEvent);
    fn emit_json(&self, event_name: &str, payload: serde_json::Value);
    fn emit_to_window_json(&self, window: &str, event_name: &str, payload: serde_json::Value);

    // --- Managed state ---

    /// Snapshot of server-configurable data (prompts, billing, thresholds, defaults).
    /// Called once per tick in the sleep loop; may be called multiple times
    /// during a run for fresh reads.
    fn server_config_snapshot(&self) -> ServerConfigSnapshot;

    // --- Outbound dependencies ---

    fn llm(&self) -> Arc<dyn LlmBackend>;
    fn stt(&self) -> Arc<dyn SttBackend>;

    // --- I/O roots ---

    fn archive_root(&self) -> PathBuf;

    // --- Clock ---

    fn now_utc(&self) -> DateTime<Utc>;
    fn now_local(&self) -> DateTime<Local>;

    /// Sleep for `dur` of virtual time. In production, forwards to tokio::time::sleep.
    /// In tests (paused-time), the same call — tokio::time::advance drives it.
    fn sleep(&self, dur: Duration) -> impl Future<Output = ()> + Send;
}

// ============================================================================
// Production impl — wraps AppHandle + real clients
// ============================================================================

#[derive(Clone)]
pub struct TauriRunContext {
    pub app: AppHandle,
    pub archive_root: PathBuf,
    pub llm: Arc<dyn LlmBackend>,
    pub stt: Arc<dyn SttBackend>,
}

impl TauriRunContext {
    pub fn new(
        app: AppHandle,
        archive_root: PathBuf,
        llm: Arc<dyn LlmBackend>,
        stt: Arc<dyn SttBackend>,
    ) -> Self {
        Self { app, archive_root, llm, stt }
    }
}

impl RunContext for TauriRunContext {
    fn emit_continuous_event(&self, event: &ContinuousModeEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("continuous_mode_event", event);
    }

    fn emit_json(&self, event_name: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.app.emit(event_name, payload);
    }

    fn emit_to_window_json(&self, window: &str, event_name: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.app.emit_to(window, event_name, payload);
    }

    fn server_config_snapshot(&self) -> ServerConfigSnapshot {
        use tauri::Manager;
        let shared = self.app.state::<SharedServerConfig>();
        // blocking_read is safe because this is called from sync contexts during
        // orchestrator init; for read-during-run we'd want read().await but
        // today's call sites all happen at snapshot points.
        shared.blocking_read().clone()
    }

    fn llm(&self) -> Arc<dyn LlmBackend> { Arc::clone(&self.llm) }
    fn stt(&self) -> Arc<dyn SttBackend> { Arc::clone(&self.stt) }

    fn archive_root(&self) -> PathBuf { self.archive_root.clone() }

    fn now_utc(&self) -> DateTime<Utc> { Utc::now() }
    fn now_local(&self) -> DateTime<Local> { Local::now() }

    fn sleep(&self, dur: Duration) -> impl Future<Output = ()> + Send {
        tokio::time::sleep(dur)
    }
}
```

**Note:** `ServerConfigSnapshot` may need to be extracted as a new type in this task. Check `src-tauri/src/server_config.rs` — if the `SharedServerConfig`'s inner type is already called something close to this (e.g., `ServerConfig`), use that directly. If it's an ad-hoc struct inside the `Arc<RwLock>`, name it here and add the `Clone` derive (should already derive it — verify). Don't let this small naming decision stall the task: pick the simplest accurate name and proceed.

- [ ] **Step 2: Wire into crate**

Edit `src-tauri/src/lib.rs`: add `pub mod run_context;`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check --lib`
Expected: compiles. Any errors here are likely type-name mismatches in the `server_config_snapshot` return type; fix by matching existing type names from `server_config.rs`.

- [ ] **Step 4: Impl check**

Append to `src-tauri/src/run_context.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_tauri_context_is_run_context() {
        fn takes_ctx(_: impl RunContext) {}
        let _ = |c: TauriRunContext| takes_ctx(c);
    }
}
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/run_context.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
refactor(harness): introduce RunContext trait + TauriRunContext

Abstracts the AppHandle + LLM + STT + clock dependencies of
run_continuous_mode behind a trait so the function can be driven
from tests without a live Tauri runtime.

Production wiring in commands/continuous.rs follows in a later commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2.2: Change `run_continuous_mode` signature

**Files:**
- Modify: `src-tauri/src/continuous_mode.rs` (line ~480)

- [ ] **Step 1: Change the signature**

At `continuous_mode.rs:480`, replace:

```rust
pub async fn run_continuous_mode(
    app: tauri::AppHandle,
    handle: Arc<ContinuousModeHandle>,
    config: Config,
    sync_ctx: ServerSyncContext,
) -> Result<(), String> {
```

with:

```rust
pub async fn run_continuous_mode<C: crate::run_context::RunContext>(
    ctx: C,
    handle: Arc<ContinuousModeHandle>,
    config: Config,
    sync_ctx: ServerSyncContext,
) -> Result<(), String> {
```

This will immediately produce ~74 compile errors, one per `app.*` / `Utc::now()` / `tokio::time::sleep` site. That's expected. Fix them in Tasks 2.3–2.5.

- [ ] **Step 2: Confirm the compile errors**

Run: `cargo check --lib 2>&1 | head -30`
Expected: errors mentioning `app` is not defined, or method called on wrong type.

- [ ] **Step 3: (No commit — commit at end of Phase 2, after all sites fixed.)**

---

### Task 2.3: Substitute event emission sites

All `app.emit(...)` / `app.emit_to(...)` calls inside `run_continuous_mode` become `ctx.emit_json(...)` / `ctx.emit_to_window_json(...)` / `ctx.emit_continuous_event(...)`.

**Files:**
- Modify: `src-tauri/src/continuous_mode.rs`

- [ ] **Step 1: Find every emit site**

```bash
grep -nE "app\.(emit|emit_to)" src-tauri/src/continuous_mode.rs
```

- [ ] **Step 2: Apply substitution pattern**

For each site:

| Current | Replacement |
|---------|-------------|
| `app.emit("continuous_mode_event", &event)` | `ctx.emit_continuous_event(&event)` |
| `app.emit("continuous_mode_event", event)` (owned) | `ctx.emit_continuous_event(&event)` |
| `app.emit("some_name", payload)` | `ctx.emit_json("some_name", serde_json::to_value(payload).unwrap_or_default())` |
| `app.emit_to("window", "event", payload)` | `ctx.emit_to_window_json("window", "event", serde_json::to_value(payload).unwrap_or_default())` |

**Important:** The `tauri::Emitter` trait import at the top of `continuous_mode.rs` becomes unused after this substitution. Remove it.

- [ ] **Step 3: Compile check**

Run: `cargo check --lib 2>&1 | grep "app\." | head -10`
Expected: no more `app.emit` / `app.emit_to` errors; remaining errors are `Utc::now` / `tokio::time::sleep` / `app.state` (handled in next tasks).

- [ ] **Step 4: (No commit — continuing.)**

---

### Task 2.4: Substitute managed-state access

Only one site today (line ~500, the `let shared = app.state::<...>()` for server config snapshot).

- [ ] **Step 1: Replace the snapshot block**

At `continuous_mode.rs:500`, replace:

```rust
let (templates, billing_data, detector_thresholds, operational) = {
    use tauri::Manager;
    let shared = app.state::<crate::commands::SharedServerConfig>();
    let sc = shared.read().await;
    let op = crate::server_config_resolve::resolve_operational(
        &config.settings,
        Some(&sc.defaults),
    );
    (
        Arc::new(sc.prompts.clone()),
        Arc::new(sc.billing.clone()),
        Arc::new(sc.thresholds.clone()),
        op,
    )
};
```

with:

```rust
let (templates, billing_data, detector_thresholds, operational) = {
    let sc = ctx.server_config_snapshot();
    let op = crate::server_config_resolve::resolve_operational(
        &config.settings,
        Some(&sc.defaults),
    );
    (
        Arc::new(sc.prompts.clone()),
        Arc::new(sc.billing.clone()),
        Arc::new(sc.thresholds.clone()),
        op,
    )
};
```

- [ ] **Step 2: Compile check**

Run: `cargo check --lib 2>&1 | grep "app\." | head -10`
Expected: no more `app.` references in `continuous_mode.rs`.

- [ ] **Step 3: (No commit.)**

---

### Task 2.5: Substitute clock sites

The biggest substitution. ~74 sites total, mostly `Utc::now()`.

**Files:**
- Modify: `src-tauri/src/continuous_mode.rs`

- [ ] **Step 1: Substitute `Utc::now()` sites**

Substitution pattern:

| Current | Replacement |
|---------|-------------|
| `Utc::now()` (inside `run_continuous_mode` body, NOT inside a closure/spawned task) | `ctx.now_utc()` |
| `Utc::now()` inside a spawned task or `async move` block where `ctx` is captured | `ctx.now_utc()` (requires cloning ctx into the block — do this at block entry) |
| `Local::now()` | `ctx.now_local()` |
| `tokio::time::sleep(dur)` (inside the `select!` branches of the main loop) | `ctx.sleep(dur)` |
| `Utc::now()` inside `impl ContinuousModeHandle` methods (lines 319-479) | **DO NOT CHANGE** — these are struct methods that don't have access to ctx. Leave as-is. |

**Capturing ctx into spawned tasks:** For any `tokio::spawn(async move { ... })` block that needs `ctx`, clone before the block:

```rust
let ctx_spawn = ctx.clone();
tokio::spawn(async move {
    // use ctx_spawn inside
});
```

- [ ] **Step 2: Do the substitution in batches**

Work top-to-bottom through `run_continuous_mode`. Recommend: substitute all `Utc::now()` first (~40 sites), then `tokio::time::sleep` (~3 sites), then verify compilation.

After each batch of ~10 substitutions:
```bash
cargo check --lib 2>&1 | grep -c "error\["
```

This gives a count of remaining errors. It should decrease monotonically as you substitute.

- [ ] **Step 3: Final compile check**

Run: `cargo check --lib`
Expected: clean compile.

- [ ] **Step 4: (No commit — one more task then commit.)**

---

### Task 2.6: Update `commands/continuous.rs` call site

**Files:**
- Modify: `src-tauri/src/commands/continuous.rs`

- [ ] **Step 1: Find the call to `run_continuous_mode`**

```bash
grep -n "run_continuous_mode" src-tauri/src/commands/continuous.rs
```

Expected: typically a single call site, passed to `tokio::spawn`.

- [ ] **Step 2: Construct `TauriRunContext` and pass it**

Replace the call site's argument pattern:

```rust
// BEFORE:
let future = crate::continuous_mode::run_continuous_mode(app.clone(), handle.clone(), config.clone(), sync_ctx.clone());
tokio::spawn(future);

// AFTER:
let archive_root = crate::local_archive::get_archive_dir()
    .map_err(|e| CommandError::from(e.to_string()))?;
let llm: Arc<dyn crate::llm_backend::LlmBackend> = Arc::new(crate::llm_client::LLMClient::new(
    /* whatever args LLMClient::new takes today — inspect and replicate */
));
let stt: Arc<dyn crate::stt_backend::SttBackend> = /* however the STT handle is obtained today */;

let ctx = crate::run_context::TauriRunContext::new(app.clone(), archive_root, llm, stt);
let future = crate::continuous_mode::run_continuous_mode(ctx, handle.clone(), config.clone(), sync_ctx.clone());
tokio::spawn(future);
```

**If the existing call site already constructs an LLMClient and STT handle**, reuse them verbatim — wrap in `Arc::new` and cast to the trait object. Don't duplicate the construction logic.

**If the existing call site constructs them lazily inside `run_continuous_mode`** (the spec notes this), then a larger edit is required: lift the construction to `commands/continuous.rs` (so the concrete types stay concrete in production, just wrapped in the trait), and remove the in-function construction from `run_continuous_mode`.

- [ ] **Step 3: Compile check**

Run: `cargo check --lib`
Expected: clean.

- [ ] **Step 4: Run full Rust test suite**

Run: `cargo test --lib`
Expected: 1,125 tests pass. Any failure here is a regression from the refactor — investigate before proceeding.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/continuous_mode.rs src-tauri/src/commands/continuous.rs
git commit -m "$(cat <<'EOF'
refactor(continuous): thread RunContext through run_continuous_mode

Change run_continuous_mode's signature from
    (AppHandle, Arc<Handle>, Config, ServerSyncContext)
to
    <C: RunContext>(C, Arc<Handle>, Config, ServerSyncContext)

All ~74 call sites of app.emit / Utc::now / Local::now / tokio::time::sleep
inside the function body now route through the context trait. Production
uses TauriRunContext (wraps AppHandle + LLMClient + STT handle), tests
will use RecordingRunContext (follows in harness/ module).

Compiler-verified no-op refactor — every test in the lib suite still
passes (1,125). Next step: half-day production run on Room 6 before
building harness code on top.

Part of continuous-mode test harness — see
docs/superpowers/specs/2026-04-18-continuous-mode-test-harness-design.md

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2.7: Phase 2 clinic verification

**This is a manual verification task for the human operator. No agentic action.**

- [ ] **Step 1: Run preflight**

From `tauri-app/`:
```bash
./scripts/preflight.sh --full
```

Expected: all 7 layers pass (same as pre-refactor baseline).

- [ ] **Step 2: Build debug app**

From `tauri-app/`:
```bash
pnpm tauri build --debug
./scripts/bundle-ort.sh "src-tauri/target/debug/bundle/macos/AMI Assist.app"
```

- [ ] **Step 3: Run a half-day production session on Room 6**

Open the app, start continuous mode, do a normal clinic session. Verify:
- Encounter detection still fires
- SOAP still generates per encounter
- Sleep mode still triggers at configured hour (test with short window if needed)
- Sensor presence still triggers splits (if sensor is wired)
- Events still reach the UI (live transcript, stats, recent encounters)
- Archive structure is unchanged

If anything is off, do NOT proceed. Investigate which `ctx.*` substitution missed or got the wrong semantic.

- [ ] **Step 4: Commit a verification note (optional)**

If you want a record of successful clinic verification:
```bash
git commit --allow-empty -m "verify: Phase 2 RunContext refactor — half-day Room 6 clinic session clean"
```

**Phase 2 done.** From here, nothing touches production code paths.

---

## Phase 3: `RecordingRunContext` + mocks + driver (1–2 days)

Build the test-side infrastructure. No comparator yet — just drive bundles through and confirm the orchestrator runs to completion without panicking.

### Task 3.1: Scaffold the harness module

**Files:**
- Create: `src-tauri/src/harness/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create module root**

Create `src-tauri/src/harness/mod.rs`:

```rust
//! Offline test harness for run_continuous_mode.
//!
//! Drives the real orchestrator function through RecordingRunContext,
//! seeded from archived replay_bundle.json data. Intended for per-encounter
//! and per-day equivalence testing before/after refactors.
//!
//! See: docs/superpowers/specs/2026-04-18-continuous-mode-test-harness-design.md

pub mod captured_event;
pub mod policies;
pub mod replay_llm_backend;
pub mod scripted_stt_backend;
pub mod scripted_sensor_source;
pub mod recording_run_context;
pub mod driver;
pub mod archive_comparator;
pub mod event_comparator;
pub mod mismatch_report;
pub mod encounter_harness;
pub mod day_harness;

pub use encounter_harness::EncounterHarness;
pub use day_harness::DayHarness;
pub use policies::{EquivalencePolicy, PromptPolicy};
pub use mismatch_report::{MismatchReport, MismatchKind};
```

- [ ] **Step 2: Wire into crate**

Edit `src-tauri/src/lib.rs`: add `pub mod harness;`.

- [ ] **Step 3: Create placeholder submodule files**

For each module listed in `harness/mod.rs`, create an empty file:
```bash
cd src-tauri/src/harness
for f in captured_event policies replay_llm_backend scripted_stt_backend scripted_sensor_source recording_run_context driver archive_comparator event_comparator mismatch_report encounter_harness day_harness; do
  echo "//! See harness/mod.rs" > "$f.rs"
done
```

- [ ] **Step 4: Compile check**

Run: `cargo check --lib`
Expected: clean (empty modules compile fine, warnings about unused `pub use` re-exports are acceptable temporarily).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/harness/ src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
chore(harness): scaffold harness module structure

Placeholder module files; implementations follow in subsequent commits.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3.2: `CapturedEvent` type + `policies.rs`

**Files:**
- Modify: `src-tauri/src/harness/captured_event.rs`
- Modify: `src-tauri/src/harness/policies.rs`

- [ ] **Step 1: Write `captured_event.rs`**

```rust
//! Event capture type for RecordingRunContext.
//!
//! Every ctx.emit_* call on a RecordingRunContext appends a CapturedEvent to
//! an internal Vec; the harness reads this Vec after the run completes to
//! build the actual-events side of the event comparator.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedEvent {
    /// When the event was emitted (ctx.now_utc at the moment of emit).
    pub virtual_ts: DateTime<Utc>,
    /// Logical window target, if any. None means app-level emit.
    pub window: Option<String>,
    /// Event name (e.g. "continuous_mode_event", "continuous_transcript_preview").
    pub event_name: String,
    /// Event payload as JSON (event_type + its fields).
    pub payload: serde_json::Value,
}

impl CapturedEvent {
    pub fn event_type(&self) -> Option<&str> {
        self.payload.get("type").and_then(|v| v.as_str())
    }
}
```

- [ ] **Step 2: Write `policies.rs`**

```rust
//! Per-test strictness policies.

#[derive(Debug, Clone, Default)]
pub enum EquivalencePolicy {
    /// Default. Compare archive state (metadata, file presence) only.
    #[default]
    ArchiveStructural,
    /// Archive-structural + emitted event sequence comparison.
    EventSequence,
}

#[derive(Debug, Clone)]
pub enum PromptPolicy {
    /// Default. LLM lookups match by (task_label, sha256(system + user)) exactly.
    /// Any mismatch surfaces as UnmatchedPrompt in the report.
    Strict,
    /// Per-task opt-out. Listed tasks replay by call-sequence order;
    /// unlisted tasks stay Strict.
    SequenceOnly { tasks: Vec<String> },
}

impl Default for PromptPolicy {
    fn default() -> Self { PromptPolicy::Strict }
}
```

- [ ] **Step 3: Unit tests**

Append to `policies.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policies_are_strict() {
        assert!(matches!(EquivalencePolicy::default(), EquivalencePolicy::ArchiveStructural));
        assert!(matches!(PromptPolicy::default(), PromptPolicy::Strict));
    }
}
```

- [ ] **Step 4: Run**

Run: `cargo test --lib harness::policies`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/harness/captured_event.rs src-tauri/src/harness/policies.rs
git commit -m "harness: CapturedEvent + EquivalencePolicy / PromptPolicy enums

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3.3: `ReplayLlmBackend`

**Files:**
- Modify: `src-tauri/src/harness/replay_llm_backend.rs`

- [ ] **Step 1: Write the failing test first (TDD)**

```rust
// src-tauri/src/harness/replay_llm_backend.rs

//! Replay LLM backend. Returns recorded responses keyed by
//! (task_label, sha256(system ++ "\n" ++ user)).

use crate::llm_backend::LlmBackend;
use crate::llm_client::CallMetrics;
use crate::replay_bundle::ReplayBundle;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::policies::PromptPolicy;

    #[tokio::test]
    async fn strict_hit_returns_recorded_response() {
        let backend = ReplayLlmBackend::for_testing(vec![
            RecordedCall {
                task_label: "encounter_detection".into(),
                system_prompt: "sys".into(),
                user_prompt: "user".into(),
                response: "RECORDED".into(),
            },
        ], PromptPolicy::Strict);

        let r = backend.generate("fast-model", "sys", "user", "encounter_detection").await;
        assert_eq!(r.unwrap(), "RECORDED");
    }

    #[tokio::test]
    async fn strict_miss_returns_error_with_task_and_hash() {
        let backend = ReplayLlmBackend::for_testing(vec![
            RecordedCall {
                task_label: "encounter_detection".into(),
                system_prompt: "sys".into(),
                user_prompt: "user".into(),
                response: "RECORDED".into(),
            },
        ], PromptPolicy::Strict);

        // Different user prompt — no recorded match.
        let r = backend.generate("fast-model", "sys", "DIFFERENT", "encounter_detection").await;
        let err = r.unwrap_err();
        assert!(err.contains("UnmatchedPrompt"), "got: {}", err);
        assert!(err.contains("encounter_detection"));
    }

    #[tokio::test]
    async fn sequence_only_task_pops_in_order() {
        let backend = ReplayLlmBackend::for_testing(vec![
            RecordedCall { task_label: "merge_check".into(), system_prompt: "a".into(), user_prompt: "b".into(), response: "FIRST".into() },
            RecordedCall { task_label: "merge_check".into(), system_prompt: "c".into(), user_prompt: "d".into(), response: "SECOND".into() },
        ], PromptPolicy::SequenceOnly { tasks: vec!["merge_check".into()] });

        let r1 = backend.generate("m", "ignored", "ignored", "merge_check").await.unwrap();
        let r2 = backend.generate("m", "also ignored", "also ignored", "merge_check").await.unwrap();
        assert_eq!(r1, "FIRST");
        assert_eq!(r2, "SECOND");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib harness::replay_llm_backend`
Expected: compile error — `RecordedCall`, `ReplayLlmBackend` don't exist.

- [ ] **Step 3: Implement**

Add above the `#[cfg(test)]` block in `replay_llm_backend.rs`:

```rust
#[derive(Debug, Clone)]
pub struct RecordedCall {
    pub task_label: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub response: String,
}

fn hash_prompt(system: &str, user: &str) -> String {
    let mut h = Sha256::new();
    h.update(system.as_bytes());
    h.update(b"\n");
    h.update(user.as_bytes());
    format!("{:x}", h.finalize())
}

pub struct ReplayLlmBackend {
    strict_map: HashMap<(String, String), String>,  // (task, prompt_hash) -> response
    seq_map: Mutex<HashMap<String, VecDeque<String>>>,  // task -> queue of responses
    sequence_only_tasks: std::collections::HashSet<String>,
}

impl ReplayLlmBackend {
    /// Build a backend from the LLM calls captured in a ReplayBundle.
    pub fn from_bundle(bundle: &ReplayBundle, policy: crate::harness::policies::PromptPolicy) -> Self {
        let mut calls: Vec<RecordedCall> = Vec::new();

        // Detection checks
        for dc in &bundle.detection_checks {
            if let Some(resp) = &dc.response_raw {
                calls.push(RecordedCall {
                    task_label: "encounter_detection".into(),
                    system_prompt: dc.prompt_system.clone(),
                    user_prompt: dc.prompt_user.clone(),
                    response: resp.clone(),
                });
            }
        }
        // Clinical check
        if let Some(_cc) = &bundle.clinical_check {
            // clinical_check doesn't currently store prompt/response in schema v3 — if added later, wire here.
            // For now, clinical content checks will be seeded via bundle.detection_checks or handled as a separate task.
        }
        // Merge check
        if let Some(mc) = &bundle.merge_check {
            if let Some(resp) = &mc.response_raw {
                calls.push(RecordedCall {
                    task_label: "merge_check".into(),
                    system_prompt: mc.prompt_system.clone(),
                    user_prompt: mc.prompt_user.clone(),
                    response: resp.clone(),
                });
            }
        }
        // Multi-patient detections
        for mp in &bundle.multi_patient_detections {
            if let Some(resp) = &mp.response_raw {
                calls.push(RecordedCall {
                    task_label: "multi_patient_detect".into(),
                    system_prompt: mp.system_prompt.clone(),
                    user_prompt: mp.user_prompt.clone(),
                    response: resp.clone(),
                });
            }
            if let Some(split) = &mp.split_decision {
                if let Some(resp) = &split.response_raw {
                    calls.push(RecordedCall {
                        task_label: "multi_patient_split".into(),
                        system_prompt: split.system_prompt.clone(),
                        user_prompt: split.user_prompt.clone(),
                        response: resp.clone(),
                    });
                }
            }
        }
        // Vision results are stored differently — their prompts aren't in the bundle today.
        // For Phase 3 we accept that vision calls may hit UnmatchedPrompt; resolve in Phase 7 by
        // extending schema or hashing text-prompt portion only.

        Self::for_testing(calls, policy)
    }

    pub fn for_testing(calls: Vec<RecordedCall>, policy: crate::harness::policies::PromptPolicy) -> Self {
        let sequence_only_tasks: std::collections::HashSet<String> = match &policy {
            crate::harness::policies::PromptPolicy::Strict => Default::default(),
            crate::harness::policies::PromptPolicy::SequenceOnly { tasks } => tasks.iter().cloned().collect(),
        };

        let mut strict_map = HashMap::new();
        let mut seq_map: HashMap<String, VecDeque<String>> = HashMap::new();

        for c in calls {
            if sequence_only_tasks.contains(&c.task_label) {
                seq_map.entry(c.task_label.clone()).or_default().push_back(c.response);
            } else {
                let hash = hash_prompt(&c.system_prompt, &c.user_prompt);
                strict_map.insert((c.task_label.clone(), hash), c.response);
            }
        }

        Self { strict_map, seq_map: Mutex::new(seq_map), sequence_only_tasks }
    }

    fn lookup(&self, task: &str, system: &str, user: &str) -> Result<String, String> {
        if self.sequence_only_tasks.contains(task) {
            let mut seq = self.seq_map.lock().expect("poisoned");
            match seq.get_mut(task).and_then(|q| q.pop_front()) {
                Some(r) => Ok(r),
                None => Err(format!("UnmatchedPrompt: task={} (SequenceOnly exhausted)", task)),
            }
        } else {
            let hash = hash_prompt(system, user);
            match self.strict_map.get(&(task.to_string(), hash.clone())) {
                Some(r) => Ok(r.clone()),
                None => Err(format!("UnmatchedPrompt: task={} hash={}", task, hash)),
            }
        }
    }
}

#[async_trait]
impl LlmBackend for ReplayLlmBackend {
    async fn generate(&self, _model: &str, system: &str, user: &str, task: &str) -> Result<String, String> {
        self.lookup(task, system, user)
    }

    async fn generate_timed(&self, model: &str, system: &str, user: &str, task: &str) -> (Result<String, String>, CallMetrics) {
        let r = self.generate(model, system, user, task).await;
        (r, CallMetrics::zero())  // Will need to confirm CallMetrics::zero() exists; else construct default manually.
    }

    async fn generate_vision(&self, _model: &str, system: &str, user: &str, _img: Vec<u8>, task: &str) -> Result<String, String> {
        // For vision: ignore image bytes in the hash for now. Phase 7 may revisit.
        self.lookup(task, system, user)
    }

    async fn generate_vision_timed(&self, model: &str, system: &str, user: &str, img: Vec<u8>, task: &str) -> (Result<String, String>, CallMetrics) {
        let r = self.generate_vision(model, system, user, img, task).await;
        (r, CallMetrics::zero())
    }
}
```

**Note on `CallMetrics::zero()`:** If this constructor doesn't exist, add it to `src-tauri/src/llm_client.rs` as a small task:

```rust
impl CallMetrics {
    pub fn zero() -> Self {
        Self {
            wall_ms: 0,
            scheduling_ms: 0,
            network_ms: 0,
            concurrent_at_start: 0,
            retry_count: 0,
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib harness::replay_llm_backend`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/harness/replay_llm_backend.rs src-tauri/src/llm_client.rs
git commit -m "harness: ReplayLlmBackend — replay LLM calls from replay_bundle

Hash-keyed (task, sha256(system+user)) lookup in Strict mode; FIFO queue
per task in SequenceOnly mode. UnmatchedPrompt surfaces as a structured
error with task label and hash for diagnostic reporting.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3.4: `ScriptedSttBackend`

**Files:**
- Modify: `src-tauri/src/harness/scripted_stt_backend.rs`

- [ ] **Step 1: Test**

```rust
// src-tauri/src/harness/scripted_stt_backend.rs

use crate::stt_backend::SttBackend;
use crate::transcription::Utterance;  // verify exact type name in Task 1.2 output
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_bundle::ReplaySegment;

    #[tokio::test]
    async fn subscribe_delivers_pre_loaded_segments_in_order() {
        let segs = vec![
            ReplaySegment { ts: "2026-04-14T10:00:00Z".into(), index: 0, start_ms: 0, end_ms: 1000, text: "hello".into(), speaker_id: None, /* fill remaining fields with defaults — see ReplaySegment definition */ ..Default::default() },
            ReplaySegment { ts: "2026-04-14T10:00:02Z".into(), index: 1, start_ms: 2000, end_ms: 3000, text: "there".into(), speaker_id: None, ..Default::default() },
        ];
        let backend = ScriptedSttBackend::from_segments(&segs);
        let mut rx = backend.subscribe();

        // Signal the backend to deliver all segments now (in the real driver
        // this is triggered by the advance loop; tests call it directly).
        backend.drain_all().await;

        let u1 = rx.recv().await.expect("got first utterance");
        let u2 = rx.recv().await.expect("got second utterance");
        // Assert u1/u2 carry the right text — exact Utterance field check
        // depends on the real Utterance struct. Adjust when wiring.
        // assert_eq!(u1.text, "hello");
        // assert_eq!(u2.text, "there");
        let _ = (u1, u2);
    }
}
```

- [ ] **Step 2: Run test — should fail compile**

Run: `cargo test --lib harness::scripted_stt_backend`
Expected: compile error.

- [ ] **Step 3: Implement**

```rust
pub struct ScriptedSttBackend {
    pending: Arc<Mutex<Vec<Utterance>>>,
    tx: mpsc::Sender<Utterance>,
    rx_slot: Arc<Mutex<Option<mpsc::Receiver<Utterance>>>>,
}

impl ScriptedSttBackend {
    pub fn from_segments(segments: &[crate::replay_bundle::ReplaySegment]) -> Self {
        let utterances = segments.iter()
            .map(|s| segment_to_utterance(s))
            .collect();
        let (tx, rx) = mpsc::channel(1024);
        Self {
            pending: Arc::new(Mutex::new(utterances)),
            tx,
            rx_slot: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// Drain all pending utterances into the channel. The driver calls this
    /// after advancing virtual time past each segment's recorded timestamp.
    /// For now (Phase 3) this is all-at-once; Phase 5/6 can refine to
    /// time-keyed delivery if it matters for detection timing.
    pub async fn drain_all(&self) {
        let pending = {
            let mut guard = self.pending.lock().expect("poisoned");
            std::mem::take(&mut *guard)
        };
        for u in pending {
            let _ = self.tx.send(u).await;
        }
    }
}

#[async_trait]
impl SttBackend for ScriptedSttBackend {
    fn subscribe(&self) -> mpsc::Receiver<Utterance> {
        self.rx_slot.lock()
            .expect("poisoned")
            .take()
            .expect("ScriptedSttBackend::subscribe called twice")
    }
}

fn segment_to_utterance(s: &crate::replay_bundle::ReplaySegment) -> Utterance {
    // Concrete conversion depends on Utterance's actual fields. Common shape:
    Utterance {
        text: s.text.clone(),
        start_ms: s.start_ms,
        end_ms: s.end_ms,
        speaker_id: s.speaker_id.clone(),
        // ... copy other fields as they exist in the production type
        ..Default::default()
    }
}
```

**Note:** Exact `Utterance` construction must match its struct definition. During this task, read `src-tauri/src/transcription.rs` to confirm field names; if `Default` is not derived, add construction explicitly.

- [ ] **Step 4: Run — should pass**

Run: `cargo test --lib harness::scripted_stt_backend`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/harness/scripted_stt_backend.rs
git commit -m "harness: ScriptedSttBackend — replay segments from bundle

Phase 3 implementation delivers all segments at once via drain_all();
Phase 5 may refine to virtual-time-keyed delivery if detection timing
depends on it.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3.5: `ScriptedSensorSource`

**Files:**
- Modify: `src-tauri/src/harness/scripted_sensor_source.rs`

- [ ] **Step 1: Read existing `MockSource`**

```bash
cat src-tauri/src/presence_sensor/sources/mock.rs
```

Note the exact `MockSource::new(...)` signature and `MockEvent` shape.

- [ ] **Step 2: Test**

```rust
// src-tauri/src/harness/scripted_sensor_source.rs

use crate::presence_sensor::sensor_source::SensorSource;
use crate::presence_sensor::sources::mock::{MockEvent, MockSource};
use crate::presence_sensor::types::SensorType;
use crate::replay_bundle::SensorTransition;
use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_from_empty_transitions() {
        let src = ScriptedSensorSource::from_transitions(&[]);
        // Assert it constructs without panic — more behavior tested in integration
        drop(src);
    }

    #[test]
    fn build_from_transitions_preserves_order() {
        let transitions = vec![
            SensorTransition { ts: "2026-04-14T10:00:00Z".into(), from: "Absent".into(), to: "Present".into() },
            SensorTransition { ts: "2026-04-14T10:15:00Z".into(), from: "Present".into(), to: "Absent".into() },
        ];
        let _src = ScriptedSensorSource::from_transitions(&transitions);
        // Event count is delegated to MockSource's internal Vec — constructor
        // success implies parse success.
    }
}
```

- [ ] **Step 3: Implement**

```rust
pub struct ScriptedSensorSource {
    inner: MockSource,
}

impl ScriptedSensorSource {
    pub fn from_transitions(transitions: &[SensorTransition]) -> Self {
        let mut events: Vec<MockEvent> = Vec::new();
        let mut last_offset = Duration::ZERO;

        let base_ts = transitions.first()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t.ts).ok())
            .map(|dt| dt.timestamp_millis());

        for t in transitions {
            let offset = if let (Some(base), Ok(dt)) = (base_ts, chrono::DateTime::parse_from_rfc3339(&t.ts)) {
                let ms = (dt.timestamp_millis() - base).max(0) as u64;
                Duration::from_millis(ms)
            } else {
                last_offset + Duration::from_secs(1) // fallback
            };
            last_offset = offset;

            let present = t.to == "Present";
            events.push(MockEvent {
                at: offset,
                // MockEvent's exact shape comes from sources/mock.rs — replace
                // the field names below to match. Common form:
                sensor: SensorType::Mmwave,
                present,
            });
        }

        // MockSource::new signature varies — this is the canonical shape
        // from presence_sensor/sources/mock.rs:40. Adjust if the actual
        // constructor takes different args.
        let inner = MockSource::new("scripted", vec![SensorType::Mmwave], events);
        Self { inner }
    }
}

impl SensorSource for ScriptedSensorSource {
    // Forward every method of SensorSource to self.inner.
    // Replicate the trait signature exactly — delegation is one-liner per method.
}
```

**The `impl SensorSource for ScriptedSensorSource` delegation:** Read `src-tauri/src/presence_sensor/sensor_source.rs` for the trait's method list. For each method, the body is `self.inner.method(args)`.

- [ ] **Step 4: Compile + test**

Run: `cargo test --lib harness::scripted_sensor_source`
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/harness/scripted_sensor_source.rs
git commit -m "harness: ScriptedSensorSource — thin wrapper over MockSource seeded from bundle

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3.6: `RecordingRunContext`

**Files:**
- Modify: `src-tauri/src/harness/recording_run_context.rs`

- [ ] **Step 1: Test**

```rust
// src-tauri/src/harness/recording_run_context.rs

use super::captured_event::CapturedEvent;
use super::replay_llm_backend::ReplayLlmBackend;
use super::scripted_stt_backend::ScriptedSttBackend;
use crate::continuous_mode_events::ContinuousModeEvent;
use crate::llm_backend::LlmBackend;
use crate::run_context::RunContext;
use crate::server_config::ServerConfigSnapshot;
use crate::stt_backend::SttBackend;
use chrono::{DateTime, Local, Utc};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emits_are_captured() {
        let ctx = RecordingRunContext::for_testing();
        ctx.emit_json("some_event", serde_json::json!({"x": 1}));
        let events = ctx.captured_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "some_event");
    }

    #[tokio::test]
    async fn virtual_clock_uses_tokio_paused_time() {
        tokio::time::pause();
        let ctx = RecordingRunContext::for_testing();
        let t0 = ctx.now_utc();
        tokio::time::advance(Duration::from_secs(60)).await;
        let t1 = ctx.now_utc();
        assert_eq!((t1 - t0).num_seconds(), 60);
    }
}
```

- [ ] **Step 2: Implement**

```rust
#[derive(Clone)]
pub struct RecordingRunContext {
    captured: Arc<Mutex<Vec<CapturedEvent>>>,
    server_config: Arc<ServerConfigSnapshot>,
    llm: Arc<dyn LlmBackend>,
    stt: Arc<dyn SttBackend>,
    archive_root: PathBuf,
    /// Virtual-time anchor. Tokio's paused clock measures elapsed from runtime start;
    /// we add that elapsed to this anchor to produce "virtual wall clock" timestamps.
    start_utc: DateTime<Utc>,
    start_instant: std::time::Instant,
}

impl RecordingRunContext {
    pub fn new(
        server_config: ServerConfigSnapshot,
        llm: Arc<dyn LlmBackend>,
        stt: Arc<dyn SttBackend>,
        archive_root: PathBuf,
        start_utc: DateTime<Utc>,
    ) -> Self {
        Self {
            captured: Arc::new(Mutex::new(Vec::new())),
            server_config: Arc::new(server_config),
            llm, stt, archive_root,
            start_utc,
            start_instant: std::time::Instant::now(),
        }
    }

    pub fn for_testing() -> Self {
        use crate::harness::policies::PromptPolicy;
        Self::new(
            ServerConfigSnapshot::compiled_defaults(),  // ensure this constructor exists; if not, construct manually
            Arc::new(ReplayLlmBackend::for_testing(vec![], PromptPolicy::Strict)),
            Arc::new(ScriptedSttBackend::from_segments(&[])),
            std::env::temp_dir().join(format!("harness-test-{}", uuid::Uuid::new_v4())),
            Utc::now(),
        )
    }

    pub fn captured_events(&self) -> Vec<CapturedEvent> {
        self.captured.lock().expect("poisoned").clone()
    }

    fn record(&self, event_name: &str, window: Option<String>, payload: serde_json::Value) {
        let virtual_ts = self.now_utc();
        self.captured.lock().expect("poisoned").push(CapturedEvent {
            virtual_ts, window, event_name: event_name.into(), payload,
        });
    }
}

impl RunContext for RecordingRunContext {
    fn emit_continuous_event(&self, event: &ContinuousModeEvent) {
        let payload = serde_json::to_value(event).unwrap_or_default();
        self.record("continuous_mode_event", None, payload);
    }

    fn emit_json(&self, event_name: &str, payload: serde_json::Value) {
        self.record(event_name, None, payload);
    }

    fn emit_to_window_json(&self, window: &str, event_name: &str, payload: serde_json::Value) {
        self.record(event_name, Some(window.into()), payload);
    }

    fn server_config_snapshot(&self) -> ServerConfigSnapshot {
        (*self.server_config).clone()
    }

    fn llm(&self) -> Arc<dyn LlmBackend> { Arc::clone(&self.llm) }
    fn stt(&self) -> Arc<dyn SttBackend> { Arc::clone(&self.stt) }

    fn archive_root(&self) -> PathBuf { self.archive_root.clone() }

    fn now_utc(&self) -> DateTime<Utc> {
        let elapsed = self.start_instant.elapsed();
        self.start_utc + chrono::Duration::from_std(elapsed).unwrap_or_default()
    }

    fn now_local(&self) -> DateTime<Local> { self.now_utc().with_timezone(&Local) }

    fn sleep(&self, dur: Duration) -> impl Future<Output = ()> + Send {
        tokio::time::sleep(dur)
    }
}
```

**Note on `ServerConfigSnapshot::compiled_defaults()`:** Confirm this exists in `server_config.rs`. If not, either add it (preferred, self-documenting) or construct a default manually in `for_testing()`.

- [ ] **Step 3: Run tests**

Run: `cargo test --lib harness::recording_run_context`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/harness/recording_run_context.rs
git commit -m "harness: RecordingRunContext — test RunContext that captures events + virtualizes clock

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3.7: Driver loop (minimal)

**Files:**
- Modify: `src-tauri/src/harness/driver.rs`

- [ ] **Step 1: Test (smoke only — no comparator yet)**

We don't yet have a `ReplayBundle::load` smoke-test fixture. Use a tiny hand-constructed bundle:

```rust
// src-tauri/src/harness/driver.rs

use crate::continuous_mode::{run_continuous_mode, ContinuousModeHandle};
use crate::config::Config;
use crate::replay_bundle::ReplayBundle;
use crate::server_sync::ServerSyncContext;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

pub async fn drive_encounter_bundle_smoke(
    bundle: &ReplayBundle,
) -> Result<super::recording_run_context::RecordingRunContext, String> {
    use super::recording_run_context::RecordingRunContext;
    use super::replay_llm_backend::ReplayLlmBackend;
    use super::scripted_stt_backend::ScriptedSttBackend;
    use crate::harness::policies::PromptPolicy;

    tokio::time::pause();

    let archive_root = tempfile::tempdir().map_err(|e| e.to_string())?.keep();
    let llm = Arc::new(ReplayLlmBackend::from_bundle(bundle, PromptPolicy::Strict));
    let stt = Arc::new(ScriptedSttBackend::from_segments(&bundle.segments));
    let server_config = crate::server_config::ServerConfigSnapshot::compiled_defaults();
    let start_utc = bundle.segments.first()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s.ts).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    let ctx = RecordingRunContext::new(
        server_config, llm, stt, archive_root, start_utc,
    );

    let handle = Arc::new(ContinuousModeHandle::new());  // confirm constructor args from continuous_mode.rs:272
    let config = Config::load_or_default();
    let sync_ctx = ServerSyncContext::noop();  // add a noop() constructor if it doesn't exist

    let ctx_for_orch = ctx.clone();
    let orch = tokio::spawn(async move {
        run_continuous_mode(ctx_for_orch, handle, config, sync_ctx).await
    });

    // Feed all segments at once (Phase 3 simplification)
    ctx.stt().subscribe();  // note: subscription is a one-shot; this is Phase 3 simplification
    // ... for a real drain-and-advance loop, see driver::drive_to_quiescence in Phase 5

    // Advance 10 minutes of virtual time to let the orchestrator process everything
    tokio::time::advance(Duration::from_secs(600)).await;

    // Stop the orchestrator
    // (ContinuousModeHandle::stop() — confirm method name)

    timeout(Duration::from_secs(5), orch).await
        .map_err(|_| "orchestrator did not stop within 5s virtual".to_string())?
        .map_err(|e| format!("orchestrator task panicked: {}", e))?
        .map_err(|e| format!("orchestrator returned error: {}", e))?;

    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn drive_trivial_bundle_does_not_panic() {
        let bundle = ReplayBundle {
            schema_version: 3,
            config: serde_json::json!({}),
            segments: vec![],
            sensor_transitions: vec![],
            vision_results: vec![],
            detection_checks: vec![],
            split_decision: None,
            clinical_check: None,
            merge_check: None,
            soap_result: None,
            multi_patient_detections: vec![],
            // ... fill remaining fields with Default where possible, or construct explicitly
            // The goal is: bundle with no inputs, orchestrator should start and stop cleanly.
        };
        let result = drive_encounter_bundle_smoke(&bundle).await;
        assert!(result.is_ok(), "smoke drive failed: {:?}", result.err());
    }
}
```

**Known gaps:** This is a smoke test only. The real driver loop (advance-and-quiesce) comes in Phase 4/5. This Phase 3 version just proves the orchestrator can be spun up with a `RecordingRunContext` without panic.

- [ ] **Step 2: Expect compile errors** — some methods (`ContinuousModeHandle::new`, `ContinuousModeHandle::stop`, `ServerSyncContext::noop`, `ServerConfigSnapshot::compiled_defaults`) may not exist. Add them as small supporting tasks — each is a constructor or no-op method.

- [ ] **Step 3: Add missing constructors as needed**

Each missing constructor gets added where it belongs:
- `ServerConfigSnapshot::compiled_defaults` in `src/server_config.rs` (builds from compiled defaults; the existing fallback path likely already does this)
- `ServerSyncContext::noop` in `src/server_sync.rs` (all fields set to no-op clients)
- `ContinuousModeHandle::new` already exists (line 272); inspect its signature and pass correct args.

- [ ] **Step 4: Run smoke test**

Run: `cargo test --lib harness::driver`
Expected: the `drive_trivial_bundle_does_not_panic` test passes. If it panics, the error message will point to the missing piece.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/harness/driver.rs src-tauri/src/server_config.rs src-tauri/src/server_sync.rs
git commit -m "harness: driver smoke test — drive run_continuous_mode from empty bundle

Phase 3 milestone: orchestrator spins up + shuts down with RecordingRunContext
without panicking. No comparator or real advance-and-quiesce loop yet —
those arrive in Phase 4/5.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3.8: Drive 3 real seed bundles

- [ ] **Step 1: Seed 3 bundles from production archive**

```bash
# From anywhere:
mkdir -p src-tauri/tests/fixtures/encounter_bundles/seed

# Pick 3 diverse bundles from the labeled days (2026-04-13, 14, 15):
# - 1 simple single-patient encounter
# - 1 encounter with merge-back
# - 1 multi-patient encounter with retrospective split

# Find candidates:
find ~/.transcriptionapp/archive/2026/04/14 -name "replay_bundle.json" | head -5
# (inspect a couple by cat'ing to check for has_soap_note, was_merged, multi_patient_detections)

# Copy selected:
cp ~/.transcriptionapp/archive/2026/04/14/<session>/replay_bundle.json \
   src-tauri/tests/fixtures/encounter_bundles/seed/2026-04-14_simple.json
# Repeat for 2 more.
```

- [ ] **Step 2: Write integration test**

Create `src-tauri/tests/harness_smoke.rs`:

```rust
//! Phase 3 smoke test — confirms the harness can drive real bundles to completion.

use transcription_app_lib::harness::driver::drive_encounter_bundle_smoke;
use transcription_app_lib::replay_bundle::ReplayBundle;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn smoke_simple_encounter() {
    let bundle_json = std::fs::read_to_string(
        "tests/fixtures/encounter_bundles/seed/2026-04-14_simple.json"
    ).expect("fixture exists");
    let bundle: ReplayBundle = serde_json::from_str(&bundle_json)
        .expect("fixture parses as ReplayBundle");

    let result = drive_encounter_bundle_smoke(&bundle).await;
    assert!(result.is_ok(), "smoke drive failed: {:?}", result.err());
}

// ... two more near-identical tests for the other seed bundles.
```

- [ ] **Step 3: Run**

Run: `cargo test --test harness_smoke -- --nocapture`
Expected: 3 smoke tests pass. Failures here mean the orchestrator tried to call something on the context that we didn't wire (e.g., a vision LLM call whose prompt isn't in the bundle → UnmatchedPrompt). Each failure is a signal about what to handle before moving to Phase 4.

- [ ] **Step 4: Address any smoke-test failures**

Common likely failures + fixes:
- **UnmatchedPrompt on vision:** vision LLM prompts aren't captured in bundle today. For Phase 3, extend `ReplayLlmBackend` to return a benign `{"name": null}` for any `vision-model` task in strict mode, logging a warning. (Properly resolved in Phase 7.)
- **Missing `ServerConfigSnapshot` field:** add the field to `compiled_defaults()`.
- **Orchestrator awaits a channel forever:** likely because `ScriptedSttBackend::drain_all` wasn't called. Call it inside the driver after `tokio::time::advance` — Phase 4 refines this.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tests/fixtures/encounter_bundles/seed/ src-tauri/tests/harness_smoke.rs
# Plus any fix commits for the issues surfaced in Step 4.
git commit -m "harness: 3 seed bundles drive to completion without panic

Phase 3 exit criterion met. Next: comparator + first-divergence + real
equivalence testing in Phase 4.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 4: Comparator + `MismatchReport` (1 day)

### Task 4.1: `MismatchReport` type

**Files:**
- Modify: `src-tauri/src/harness/mismatch_report.rs`

- [ ] **Step 1: Test**

```rust
// src-tauri/src/harness/mismatch_report.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum MismatchKind {
    DetectionDecision { at_segment_index: u64, expected: serde_json::Value, actual: serde_json::Value },
    MergeDecision { expected: serde_json::Value, actual: serde_json::Value },
    MultiPatientSplit { expected: serde_json::Value, actual: serde_json::Value },
    ArchiveField { session: String, field: String, expected: serde_json::Value, actual: serde_json::Value },
    MissingArchiveFile { session: String, file: String },
    UnexpectedArchiveFile { session: String, file: String },
    EventPayload { event_index: usize, field: String, expected: serde_json::Value, actual: serde_json::Value },
    MissingEvent { expected_event_name: String, at_event_index: usize },
    UnexpectedEvent { actual_event_name: String, at_event_index: usize },
    UnmatchedPrompt { task: String, prompt_hash: String },
    OrchestratorPanic { message: String },
    OrchestratorTimeout { limit_secs: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MismatchReport {
    pub test_id: String,
    pub bundle_path: String,
    pub verdict: Verdict,
    pub first_divergence: Option<Divergence>,
    pub summary_one_liner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Verdict { Equivalent, Divergent }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Divergence {
    pub kind: MismatchKind,
    pub preceding_events: Vec<super::captured_event::CapturedEvent>,
    pub drill_in_command: String,
}

impl MismatchReport {
    pub fn equivalent(test_id: &str, bundle_path: &str) -> Self {
        Self {
            test_id: test_id.into(),
            bundle_path: bundle_path.into(),
            verdict: Verdict::Equivalent,
            first_divergence: None,
            summary_one_liner: format!("{}: Equivalent", test_id),
        }
    }

    pub fn divergent(test_id: &str, bundle_path: &str, kind: MismatchKind, preceding: Vec<super::captured_event::CapturedEvent>) -> Self {
        let summary = summarize_one_liner(test_id, &kind);
        let drill_in = format!(
            "HARNESS_FOCUS=1 cargo test --test harness_per_encounter {} -- --nocapture",
            test_id
        );
        Self {
            test_id: test_id.into(),
            bundle_path: bundle_path.into(),
            verdict: Verdict::Divergent,
            first_divergence: Some(Divergence { kind, preceding_events: preceding, drill_in_command: drill_in }),
            summary_one_liner: summary,
        }
    }
}

fn summarize_one_liner(test_id: &str, kind: &MismatchKind) -> String {
    match kind {
        MismatchKind::DetectionDecision { at_segment_index, .. } =>
            format!("{}: detection decision differs at segment {}", test_id, at_segment_index),
        MismatchKind::ArchiveField { session, field, .. } =>
            format!("{}: archive field '{}' differs for session {}", test_id, field, session),
        MismatchKind::MissingArchiveFile { session, file } =>
            format!("{}: expected file {} missing from session {}", test_id, file, session),
        MismatchKind::UnmatchedPrompt { task, prompt_hash } =>
            format!("{}: no recorded response for task '{}' prompt_hash={}", test_id, task, prompt_hash),
        MismatchKind::OrchestratorPanic { message } =>
            format!("{}: orchestrator panicked — {}", test_id, message),
        _ => format!("{}: divergence — see JSON report", test_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_report_serializes_cleanly() {
        let r = MismatchReport::equivalent("test_001", "path/to/bundle.json");
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("Equivalent"));
    }

    #[test]
    fn divergent_report_has_summary_with_segment() {
        let r = MismatchReport::divergent(
            "test_001", "bundle.json",
            MismatchKind::DetectionDecision {
                at_segment_index: 42,
                expected: serde_json::json!({"complete": true}),
                actual: serde_json::json!({"complete": false}),
            },
            vec![],
        );
        assert!(r.summary_one_liner.contains("segment 42"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib harness::mismatch_report`
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/harness/mismatch_report.rs
git commit -m "harness: MismatchReport + MismatchKind taxonomy

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4.2: `ArchiveComparator`

**Files:**
- Modify: `src-tauri/src/harness/archive_comparator.rs`

- [ ] **Step 1: Test**

```rust
// src-tauri/src/harness/archive_comparator.rs

use super::mismatch_report::MismatchKind;
use crate::replay_bundle::ReplayBundle;
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;

    use crate::replay_bundle::{Outcome, ReplayBundle};
    use tempfile::tempdir;

    fn empty_bundle_with_outcome(outcome: Outcome) -> ReplayBundle {
        ReplayBundle {
            schema_version: 3,
            config: serde_json::json!({}),
            segments: vec![],
            sensor_transitions: vec![],
            vision_results: vec![],
            detection_checks: vec![],
            split_decision: None,
            clinical_check: None,
            merge_check: None,
            soap_result: None,
            multi_patient_detections: vec![],
            name_tracker: None,
            outcome: Some(outcome),
        }
    }

    fn write_session_meta(root: &std::path::Path, session_id: &str, meta: serde_json::Value) {
        // Writes archive_root/2026/04/14/<session_id>/metadata.json
        let dir = root.join("2026").join("04").join("14").join(session_id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("metadata.json"), serde_json::to_string(&meta).unwrap()).unwrap();
    }

    #[test]
    fn equivalent_when_metadata_matches_expected_allowlist() {
        let outcome = Outcome {
            session_id: "abc123".into(),
            encounter_number: 1,
            word_count: 500,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            has_soap_note: Some(true),
        };
        let bundle = empty_bundle_with_outcome(outcome);
        let root = tempdir().unwrap();
        write_session_meta(root.path(), "abc123", serde_json::json!({
            "encounter_number": 1,
            "has_soap_note": true,
            "was_merged": false,
            "merged_into": null,
            "patient_name": "Doe, Jane",
            "charting_mode": "continuous",
            "detection_method": "llm",
            "has_billing_record": true,
            "first_segment_index": 0,
            "last_segment_index": 42,
            "likely_non_clinical": false,
            "encounter_started_at": "2026-04-14T10:00:00Z",
        }));

        let cmp = ArchiveComparator::default();
        let mismatches = cmp.compare(&bundle, root.path()).unwrap();
        assert!(mismatches.is_empty(), "expected equivalent, got: {:?}", mismatches);
    }

    #[test]
    fn archive_field_mismatch_returned_with_session_and_field() {
        let outcome = Outcome {
            session_id: "abc123".into(),
            encounter_number: 1,
            word_count: 500,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            has_soap_note: Some(true),
        };
        let bundle = empty_bundle_with_outcome(outcome);
        let root = tempdir().unwrap();
        write_session_meta(root.path(), "abc123", serde_json::json!({
            "encounter_number": 2,   // expected 1 — divergence
            "has_soap_note": true,
            "was_merged": false,
        }));

        let cmp = ArchiveComparator::default();
        let mismatches = cmp.compare(&bundle, root.path()).unwrap();
        assert_eq!(mismatches.len(), 1);
        match &mismatches[0] {
            MismatchKind::ArchiveField { session, field, .. } => {
                assert_eq!(session, "abc123");
                assert_eq!(field, "encounter_number");
            }
            other => panic!("expected ArchiveField, got {:?}", other),
        }
    }

    #[test]
    fn missing_session_reported() {
        let outcome = Outcome {
            session_id: "not_on_disk".into(),
            encounter_number: 1,
            word_count: 0, is_clinical: true, was_merged: false, merged_into: None, has_soap_note: None,
        };
        let bundle = empty_bundle_with_outcome(outcome);
        let root = tempdir().unwrap();
        // Deliberately don't write any session dir.

        let cmp = ArchiveComparator::default();
        let err = cmp.compare(&bundle, root.path()).unwrap_err();
        assert!(err.contains("not_on_disk"));
    }

    #[test]
    fn ignores_non_allowlist_fields() {
        let outcome = Outcome {
            session_id: "abc123".into(),
            encounter_number: 1,
            word_count: 500, is_clinical: true, was_merged: false, merged_into: None, has_soap_note: Some(true),
        };
        let bundle = empty_bundle_with_outcome(outcome);
        let root = tempdir().unwrap();
        // Different encounter_started_at + session_duration (both NOT in allowlist).
        write_session_meta(root.path(), "abc123", serde_json::json!({
            "encounter_number": 1,
            "has_soap_note": true,
            "was_merged": false,
            "encounter_started_at": "9999-01-01T00:00:00Z",   // wildly different timestamp — should be ignored
            "session_duration_ms": 12345,                      // also not in allowlist
        }));

        let cmp = ArchiveComparator::default();
        let mismatches = cmp.compare(&bundle, root.path()).unwrap();
        assert!(mismatches.is_empty(), "allowlist should ignore ts/duration fields, got: {:?}", mismatches);
    }
}
```

Fill in test bodies with concrete construction when implementing. Use `tempfile::tempdir()`.

- [ ] **Step 2: Implement**

```rust
pub struct ArchiveComparator {
    /// Fields we compare from metadata.json. Everything not in this list is ignored.
    allowlist: Vec<&'static str>,
    /// Files that must exist if the bundle says they should.
    expected_files: Vec<&'static str>,
}

impl Default for ArchiveComparator {
    fn default() -> Self {
        Self {
            allowlist: vec![
                "charting_mode", "encounter_number", "detection_method",
                "patient_name", "has_soap_note", "has_billing_record",
                "first_segment_index", "last_segment_index", "likely_non_clinical",
                "was_merged", "merged_into",
            ],
            expected_files: vec![
                // Conditional: only required if bundle says has_soap_note=true, etc.
                // Handled in compare() per-session.
            ],
        }
    }
}

impl ArchiveComparator {
    /// Compare the actual archive at `archive_root` against what the bundle says
    /// should exist. Returns Ok(vec![]) on equivalence; Err on unrelated failures;
    /// Ok(vec![...]) with mismatches in canonical order otherwise.
    pub fn compare(&self, bundle: &ReplayBundle, archive_root: &Path) -> Result<Vec<MismatchKind>, String> {
        let mut mismatches = Vec::new();

        // Walk expected sessions (from bundle.outcome).
        let outcome = bundle.outcome.as_ref().ok_or("bundle has no outcome")?;

        // Derive expected session dir from bundle
        let session_id = &outcome.session_id;
        let actual_meta_path = find_session_meta(archive_root, session_id)
            .ok_or(format!("session {} not found in actual archive", session_id))?;
        let actual_meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&actual_meta_path).map_err(|e| e.to_string())?
        ).map_err(|e| e.to_string())?;

        let expected_meta = build_expected_meta_from_outcome(outcome);

        // Walk allowlist fields
        for field in &self.allowlist {
            let expected = expected_meta.get(field).cloned().unwrap_or(serde_json::Value::Null);
            let actual = actual_meta.get(field).cloned().unwrap_or(serde_json::Value::Null);
            if expected != actual {
                mismatches.push(MismatchKind::ArchiveField {
                    session: session_id.clone(),
                    field: field.to_string(),
                    expected, actual,
                });
                return Ok(mismatches);  // first-divergence
            }
        }

        // Check conditional files
        if outcome.was_merged {
            // Expect a sibling replay_bundle.merged_<short_id>.json under surviving session.
            // Check if it exists.
            // If missing, add MissingArchiveFile.
        }

        Ok(mismatches)
    }
}

fn find_session_meta(archive_root: &Path, session_id: &str) -> Option<std::path::PathBuf> {
    // Walk YYYY/MM/DD subdirs looking for a directory named session_id.
    // Return <that dir>/metadata.json if it exists.
    walkdir::WalkDir::new(archive_root)
        .into_iter()
        .flatten()
        .find(|e| e.file_name() == session_id && e.file_type().is_dir())
        .map(|e| e.path().join("metadata.json"))
}

fn build_expected_meta_from_outcome(outcome: &crate::replay_bundle::Outcome) -> serde_json::Value {
    serde_json::json!({
        "encounter_number": outcome.encounter_number,
        "has_soap_note": outcome.has_soap_note,  // confirm field exists; if not, derive from outcome.soap_result
        "was_merged": outcome.was_merged,
        "merged_into": outcome.merged_into,
        // ...map other allowlist fields from Outcome
    })
}
```

**Note on `walkdir`:** Confirm it's in Cargo.toml. If not, add it as a dev-dependency (since this code is test-only, technically — but the harness module is in `src/`, so it's a runtime dep).

- [ ] **Step 3: Run**

Run: `cargo test --lib harness::archive_comparator`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/harness/archive_comparator.rs src-tauri/Cargo.toml
git commit -m "harness: ArchiveComparator — allowlist-based metadata diff with first-divergence

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4.3: `EncounterHarness` wire-up

**Files:**
- Modify: `src-tauri/src/harness/encounter_harness.rs`

- [ ] **Step 1: Implement**

```rust
// src-tauri/src/harness/encounter_harness.rs

use super::archive_comparator::ArchiveComparator;
use super::driver::drive_encounter_bundle_smoke;
use super::mismatch_report::{MismatchReport, MismatchKind, Verdict};
use super::policies::{EquivalencePolicy, PromptPolicy};
use crate::replay_bundle::ReplayBundle;
use std::path::PathBuf;

pub struct EncounterHarness {
    bundle: ReplayBundle,
    bundle_path: PathBuf,
    test_id: String,
    equiv_policy: EquivalencePolicy,
    prompt_policy: PromptPolicy,
}

impl EncounterHarness {
    pub fn new(bundle_path: impl Into<PathBuf>) -> Self {
        let bundle_path = bundle_path.into();
        let bundle_json = std::fs::read_to_string(&bundle_path)
            .expect("bundle path exists");
        let bundle: ReplayBundle = serde_json::from_str(&bundle_json)
            .expect("bundle parses");
        let test_id = bundle_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("encounter")
            .to_string();

        Self {
            bundle, bundle_path, test_id,
            equiv_policy: Default::default(),
            prompt_policy: Default::default(),
        }
    }

    pub fn with_policy(mut self, p: EquivalencePolicy) -> Self { self.equiv_policy = p; self }
    pub fn with_prompt_policy(mut self, p: PromptPolicy) -> Self { self.prompt_policy = p; self }

    pub async fn run(self) -> MismatchReport {
        let path_str = self.bundle_path.to_string_lossy().to_string();

        // Drive the orchestrator
        let ctx = match drive_encounter_bundle_smoke(&self.bundle).await {
            Ok(c) => c,
            Err(e) => {
                return MismatchReport::divergent(
                    &self.test_id, &path_str,
                    MismatchKind::OrchestratorPanic { message: e },
                    vec![],
                );
            }
        };

        // Compare archive
        let comparator = ArchiveComparator::default();
        match comparator.compare(&self.bundle, &ctx.archive_root()) {
            Ok(mismatches) if mismatches.is_empty() => {
                MismatchReport::equivalent(&self.test_id, &path_str)
            }
            Ok(mut mismatches) => {
                let kind = mismatches.remove(0);
                let preceding = ctx.captured_events()
                    .into_iter().rev().take(3).rev().collect();
                MismatchReport::divergent(&self.test_id, &path_str, kind, preceding)
            }
            Err(e) => {
                MismatchReport::divergent(
                    &self.test_id, &path_str,
                    MismatchKind::OrchestratorPanic { message: e },
                    vec![],
                )
            }
        }
    }
}

// Sugar for tests:
impl MismatchReport {
    /// Panic with summary + write JSON artifact if Divergent.
    pub fn expect_equivalent(self) {
        if let Verdict::Divergent = self.verdict {
            // Write full JSON artifact
            let artifact_dir = std::path::PathBuf::from("target/harness-report");
            let _ = std::fs::create_dir_all(&artifact_dir);
            let path = artifact_dir.join(format!("{}.json", self.test_id));
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&self).unwrap_or_default());
            eprintln!("{}", self.summary_one_liner);
            eprintln!("Full report: {}", path.display());
            if let Some(d) = &self.first_divergence {
                eprintln!("Drill-in: {}", d.drill_in_command);
            }
            panic!("harness detected divergence: {}", self.summary_one_liner);
        }
    }
}
```

- [ ] **Step 2: Write integration test**

Replace `src-tauri/tests/harness_smoke.rs` with `src-tauri/tests/harness_per_encounter.rs`:

```rust
use transcription_app_lib::harness::EncounterHarness;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn encounter_seed_01_simple() {
    EncounterHarness::new("tests/fixtures/encounter_bundles/seed/2026-04-14_simple.json")
        .run()
        .await
        .expect_equivalent();
}

// Repeat for other 2 seeds.
```

- [ ] **Step 3: Run**

Run: `cargo test --test harness_per_encounter -- --nocapture`
Expected: 3 tests pass against their own recorded baselines.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/harness/encounter_harness.rs src-tauri/tests/harness_per_encounter.rs
git rm src-tauri/tests/harness_smoke.rs
git commit -m "harness: EncounterHarness runner + expect_equivalent with JSON artifact

Phase 4 exit criterion met — 3 seed bundles pass equivalence check
against their own baselines.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 5: EventComparator + deliberate-regression self-test (0.5 day)

### Task 5.1: `EventComparator`

**Files:**
- Modify: `src-tauri/src/harness/event_comparator.rs`

- [ ] **Step 1: Test + Implement**

Same pattern as `ArchiveComparator`. Walks expected events (reconstructed from bundle: `detection_check` → `continuous_mode_event {type: "encounter_detected"}`, `merge_check` → `encounter_merged`, etc.) vs `ctx.captured_events()`. Compares `event_type` + key payload fields; ignores timestamps.

Skeleton:

```rust
pub struct EventComparator {
    payload_allowlist: std::collections::HashMap<String, Vec<&'static str>>,  // event_type -> fields to check
}

impl EventComparator {
    pub fn compare(&self, expected: &[ExpectedEvent], actual: &[crate::harness::captured_event::CapturedEvent])
        -> Result<Vec<crate::harness::mismatch_report::MismatchKind>, String>
    {
        // Parallel walk; first index where event_type or allowlisted payload field
        // differs → push MismatchKind::EventPayload / MissingEvent / UnexpectedEvent, return.
        // ...
    }
}
```

Helpers:
- `reconstruct_expected_events_from_bundle(bundle: &ReplayBundle) -> Vec<ExpectedEvent>` — walks bundle's decisions + outcomes to synthesize the events the orchestrator would have emitted if behavior matches.

- [ ] **Step 2: Wire into EncounterHarness when `EquivalencePolicy::EventSequence` is selected**

Inside `EncounterHarness::run`:
```rust
if matches!(self.equiv_policy, EquivalencePolicy::EventSequence) {
    let ev_mismatches = EventComparator::default().compare(
        &reconstruct_expected_events_from_bundle(&self.bundle),
        &ctx.captured_events(),
    )?;
    // First-divergence walk chooses event-level vs archive-level errors first
}
```

- [ ] **Step 3: Test + commit**

---

### Task 5.2: Deliberate-regression self-test

The key check that the harness actually works.

- [ ] **Step 1: Introduce a known-bad change**

In `run_continuous_mode`, find where the confidence gate is applied (around `base_threshold` in the merge-back-count adjustment). Change `base_threshold` to always be `0.1` (guaranteed to split on any detection).

- [ ] **Step 2: Run harness**

Run: `cargo test --test harness_per_encounter -- --nocapture`
Expected: at least one test fails with a `MismatchKind::DetectionDecision` or `ArchiveField` (encounter count off). The summary one-liner should clearly indicate where behavior diverged.

- [ ] **Step 3: Confirm the failure report is actionable**

Read the stderr summary. It should name the bundle, the segment, what decision flipped, and point to the JSON artifact. If any of those are missing/wrong, fix `MismatchReport::summary_one_liner` or the first-divergence walk.

- [ ] **Step 4: Revert the bad change**

`git checkout src-tauri/src/continuous_mode.rs`

- [ ] **Step 5: Confirm all tests pass again**

Run: `cargo test --test harness_per_encounter`
Expected: green.

- [ ] **Step 6: Commit a note**

```bash
git commit --allow-empty -m "verify: deliberate-regression self-test — harness correctly detects a known-bad threshold change"
```

---

## Phase 6: DayHarness (1 day)

### Task 6.1: `DayHarness` driver

**Files:**
- Modify: `src-tauri/src/harness/day_harness.rs`

Pattern: load `day_log.jsonl` + walk `YYYY/MM/DD/<session>/replay_bundle.json` files. Drive them through one orchestrator run, ordered by encounter start time. Virtual clock jumps over idle gaps between encounters.

(Skeleton — ~150 lines of similar structure to `EncounterHarness` + cross-encounter state).

### Task 6.2: Cross-encounter invariants

Add a `DayInvariantsChecker` that walks actual-archive day state + `ctx.captured_events` and asserts:
- `recent_encounters` list contains no merged-away session IDs (walk continuous_mode_event payloads)
- Sleep-mode events paired
- No orphan SOAP (every expected-has-SOAP session has the file)
- Merge-back produces sibling bundles
- Retrospective multi-patient split fires when warranted (compare `multi_patient_detections.split_decision.decision_at_stage == Retrospective` counts)
- Day log rotation fires correctly if the day crosses midnight

### Task 6.3: Bootstrap 3 day fixtures + write per-day integration test

### Task 6.4: Verify 3 day tests pass

---

## Phase 7: Fixture tooling + corpus expansion (0.5 day)

### Task 7.1: `bootstrap_harness_fixture` CLI

Register as `[[bin]]` in `Cargo.toml`. Subcommands:
- `--from-session YYYY-MM-DD/session_id` — copy one encounter bundle
- `--from-day YYYY-MM-DD` — copy all encounters in a day
- `--label <name>` — override the fixture label

After copy, runs the harness once against the new fixture; only commits the fixture if the harness returns Equivalent. Prevents bad baselines from landing.

### Task 7.2: `UPDATE_HARNESS_BASELINES=1` env-var flow

When set, divergences update the expected values in the bundle in place (rewriting the `outcome`, `split_decision`, etc. from actual results). After the update, the test passes. Engineer reviews `git diff tests/fixtures/` and commits if intended.

### Task 7.3: Expand corpus

Bootstrap 20+ encounter fixtures covering Section 4's diversity list from the spec. Bootstrap all 6 labeled days as day fixtures.

---

## Phase 8: Preflight wiring + docs (0.5 day)

### Task 8.1: Add harness tier to preflight

Edit `tauri-app/scripts/preflight.sh`. Add (after the existing layer invocations):

```bash
echo "== Layer 8: harness per-encounter =="
cd "$(dirname "$0")/../src-tauri"
cargo test --test harness_per_encounter --release 2>&1 | tee /tmp/harness_per_encounter.log
grep -q "^test result: ok" /tmp/harness_per_encounter.log || exit 1
```

### Task 8.2: Update `docs/TESTING.md`

Add Layer 8 to the "At a glance" table + a new "Layer 8: Orchestrator equivalence harness" section describing what it catches, how to add a fixture, and how to read a failure report.

---

## Success criteria (verify all six)

After Phase 8 completes:

1. [ ] ≥20 encounter bundles committed under `tests/fixtures/encounter_bundles/`, spanning the diversity list in the spec
2. [ ] ≥3 day fixtures under `tests/fixtures/days/`
3. [ ] `cargo test --test harness_per_encounter` passes on `main`
4. [ ] Deliberate-regression self-test: re-introduce a known-bad change, confirm harness catches it and the report points to the right encounter/segment/kind. Revert. *(Documented outcome, not a committed test.)*
5. [ ] `cargo test --test harness_per_encounter` completes in <60s on Room 6 dev machine
6. [ ] `scripts/preflight.sh --full` includes the harness tier; `docs/TESTING.md` documents Layer 8

If all six are satisfied, the harness is ready for its purpose: serving as the safety net for the `run_continuous_mode` body decomposition (next spec).
