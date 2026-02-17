# Transcription App — Detailed Code Review & Next-Level Recommendations

> **Historical Document (2026-01-15)**: This review predates the code-first review in [CODE_REVIEW_FINDINGS.md](../CODE_REVIEW_FINDINGS.md) (2026-02-06, updated 2026-02-17). Several findings below have been addressed in subsequent commits. Refer to CODE_REVIEW_FINDINGS.md for the current resolution status. This document is retained for its security-focused analysis (P0 items) which was intentionally out of scope for the later review.

Date: 2026-01-15
Reviewed repo: `/Users/backoffice/transcriptionapp` (focus: `tauri-app/` and `tauri-app/src-tauri/`)

This review is based on reading and cross-referencing the actual codepaths (not docs). I traced the main flows end-to-end: UI → Tauri IPC → pipeline threads → network integrations (Whisper server, LLM router, Medplum) → persistence (config/auth/logs/audio/debug storage).

### Findings resolved since this review (as of 2026-02-17)

- **P0-2 Debug storage default=true**: FIXED — now `cfg!(debug_assertions)` (only enabled in debug builds)
- **P0-4 Default API key shipped**: FIXED — `llm_api_key` now defaults to empty string
- **P0-4 Default Medplum URL**: FIXED — `medplum_server_url` now defaults to empty string
- **P1-2 Hard-coded "fast-model"**: FIXED — `LLMClient` now accepts `fast_model` parameter, used in greeting detection
- **P1-4 Medplum SOAP uses placeholder IDs**: FIXED — patient ID now resolved from encounter `subject.reference`
- **P1-Reliability-2 Thread lifecycle (stop without join)**: FIXED — Pipeline handle join in background `std::thread::spawn` / `spawn_blocking`
- **Test results**: Now 421 Rust tests passing (0 failed), 387 frontend tests passing

---

## Executive Summary

The app is structurally impressive: clear module boundaries on the Rust side (`pipeline`, `vad`, `listening`, `llm_client`, `medplum`), a reasonably well-structured React UI with hooks, and a serious test posture (unit + property-based + stress/soak scaffolding). There is also an auditing-oriented logging module.

However, there are multiple **P0 (critical)** issues that make the current build unsafe for real clinical deployment:

- **PHI leaks into logs and console output** (directly contradicts the “never log PHI” rule in `activity_log.rs`).
- **Secrets and tokens are stored unencrypted on disk**, and may be written with permissive file modes depending on umask.
- **Debug storage that explicitly saves PHI (transcripts/audio/SOAP) is enabled by default**.
- Several defaults hard-code internal IPs and a default API key.

These items are solvable, but they must be addressed before calling the app “production-ready” in a clinical setting.

---

## Local Verification I Ran (in this environment)

> **Updated 2026-02-17**: Current test results are significantly improved from the original review.

As of 2026-02-17:
- `pnpm -C tauri-app typecheck` — 0 errors
- `pnpm -C tauri-app lint` — 2 errors, 111 warnings
- `pnpm -C tauri-app test:run` — 387 passed, 6 skipped
- `cargo test --lib` — 421 passed, 0 failed
- `cargo check` — 0 warnings

Original results (2026-01-15):

Frontend (`tauri-app/`):
- `pnpm -C tauri-app typecheck` ✅
- `pnpm -C tauri-app lint` ✅ (warnings: widespread `console.*` usage + a few React hook dependency warnings)
- `pnpm -C tauri-app test:run` ✅ (21 files, 427 tests)

Rust (`tauri-app/src-tauri/`):
- `cargo test` ❌ on this machine (308 passed, 6 failed, 16 ignored)
  - `llm_client` / `medplum` / `whisper_server` "client creation" tests panic inside `system-configuration` (`Attempted to create a NULL object.`). This is consistent with macOS system proxy discovery failing in some environments when constructing `reqwest::Client`.
  - `audio::tests::test_get_device_default` fails because CPAL returns a device but `device.name()` can error in restricted environments.

Recommendations from these results:
- Build `reqwest::Client` with `.no_proxy()` (or otherwise disable implicit macOS proxy discovery) in `LLMClient`, `WhisperServerClient`, and `MedplumClient`. This is also a security win: you don’t want PHI silently routed via a system proxy.
- Make audio-hardware-dependent tests fully conditional/ignored (or mock CPAL) so CI and headless environments don’t fail nondeterministically.

---

## Verified Architecture & End-to-End Flows (What’s Really Implemented)

### UI modes → Session pipeline
- UI triggers session start/stop/reset through IPC (`invoke('start_session')`, etc.) in `tauri-app/src/hooks/useSessionState.ts`.
- Rust `start_session` spawns the audio pipeline thread (`tauri-app/src-tauri/src/commands/session.rs` → `tauri-app/src-tauri/src/pipeline.rs`).
- Pipeline emits events back to UI:
  - `session_status` / `transcript_update` from `tauri-app/src-tauri/src/commands/session.rs`
  - `biomarker_update` / `audio_quality` from `tauri-app/src-tauri/src/pipeline.rs`

### Auto-session detection (“listening mode”)
- UI toggles listening mode via IPC (`start_listening`, `stop_listening`) in `tauri-app/src/hooks/useAutoDetection.ts`.
- Rust `start_listening` spawns a listening thread and emits `listening_event` (see `tauri-app/src-tauri/src/commands/listening.rs` and `tauri-app/src-tauri/src/listening.rs`).
- Listening mode uses **remote Whisper server + LLM** to classify greetings, and supports “optimistic recording” by buffering initial audio and handing it to `start_session`.

### Transcription provider reality check
- The pipeline is **hard-coded to remote Whisper server** (see `tauri-app/src-tauri/src/pipeline.rs` where `TranscriptionProvider::Remote` is always used).
- A local Whisper provider (`whisper-rs`) exists in the codebase (`tauri-app/src-tauri/src/transcription.rs`) but is currently dead/inactive in the main pipeline.
- Frontend settings also indicate remote-only (`whisper_mode: 'remote'` in `tauri-app/src/hooks/useSettings.ts`).

### EMR (Medplum) sync
- OAuth PKCE flow: `tauri-app/src/components/AuthProvider.tsx` → IPC → `tauri-app/src-tauri/src/commands/medplum.rs` → `tauri-app/src-tauri/src/medplum.rs`.
- Data operations: patient search, encounter create/complete, document/audio upload, history fetch.

---

## P0 — Security/Privacy/Compliance Gaps (Fix First)

### 1) PHI is logged (multiple confirmed violations)

This is the highest priority because logs are persisted to `~/.transcriptionapp/logs/activity.log.*` by default.

Confirmed examples:
- **Transcript text logged** during greeting analysis:
  - `tauri-app/src-tauri/src/listening.rs:543` logs `result.transcript`
  - `tauri-app/src-tauri/src/listening.rs:635` logs full `transcript`
- **Patient search query logged** (name/MRN is PHI):
  - `tauri-app/src-tauri/src/commands/medplum.rs:191` logs `query`
- **OAuth deep link URL logged with code/state**:
  - `tauri-app/src-tauri/src/lib.rs:162` logs full `argv` (may include the deep link URL)
  - `tauri-app/src-tauri/src/lib.rs:176` logs the full deep link string
  - `tauri-app/src/components/AuthProvider.tsx:63`, `tauri-app/src/components/AuthProvider.tsx:95` logs deep link URLs to console (includes OAuth code/state)

Recommendations:
- Create a hard rule: **no transcript text, SOAP text, patient identifiers, patient names, MRNs, OAuth codes, access/refresh tokens** in logs at any level.
- Add a dedicated sanitization/redaction helper and use it everywhere:
  - Strip query params from URLs before logging.
  - Log counts/lengths/IDs only.
- Add a “PHI logging regression test”:
  - A unit test that asserts log messages produced by key flows do not contain obvious sensitive markers (`code=`, `state=`, `Bearer `, etc.).
  - Consider a build feature flag like `phi_logging` that is **off** by default.

### 2) Debug storage (explicit PHI) is enabled by default

- `tauri-app/src-tauri/src/config.rs:63-65` sets `debug_storage_enabled` default to `true` with a comment “DISABLE IN PRODUCTION”.
- `tauri-app/src-tauri/src/commands/session.rs` writes transcript segments + copies audio to debug storage when enabled.
- `tauri-app/src-tauri/src/debug_storage.rs` documents it stores transcript/audio/SOAP in `~/.transcriptionapp/debug/<session-id>/`.

Recommendations:
- Default `debug_storage_enabled` to `false` and require an explicit opt-in.
- Consider compile-time gating:
  - e.g., only allow debug storage in `debug_assertions` or behind a `debug-storage` Cargo feature.
- If debug storage remains, add:
  - clear UI indicator when enabled
  - retention policy + “Delete all local data” control
  - encryption-at-rest (see next point)

### 3) Secrets & tokens are stored unencrypted on disk (and may be world-readable)

Confirmed:
- Medplum auth state is written to `~/.transcriptionapp/medplum_auth.json`:
  - `tauri-app/src-tauri/src/medplum.rs:96-113` (`save_to_file`)
- App config is written to `~/.transcriptionapp/config.json`:
  - `tauri-app/src-tauri/src/config.rs:479-489` (`Config::save`)
- Settings include `llm_api_key` and OAuth tokens include `access_token`/`refresh_token`.

Risks:
- Depending on OS defaults/umask, these files may be readable by other local users.
- Tokens and API keys in plaintext are not acceptable for clinical workflows.

Recommendations:
- Move secrets into OS-native secure storage:
  - macOS Keychain, Windows Credential Manager, Linux Secret Service.
  - In Tauri ecosystem, consider `tauri-plugin-stronghold` or an OS keychain plugin.
- Store only non-secret config in `config.json`.
- Ensure any on-disk PHI artifacts (recordings, debug storage, caches) are either:
  - encrypted at rest, or
  - optional + clearly user-controlled, with strict permissions.
- If any sensitive files must exist, explicitly set permissions (Unix `0600`) and use atomic writes.

### 4) Insecure defaults (hard-coded internal services + default API key)

Confirmed:
- `tauri-app/src-tauri/src/config.rs:71-73` default `llm_api_key` is `"ai-scribe-key"`.
- `tauri-app/src-tauri/src/config.rs:109-120` defaults point to `http://172.16.100.45:8001` (Whisper) and `http://172.16.100.45:8103` (Medplum).

Recommendations:
- Defaults should be safe and environment-agnostic:
  - Prefer empty values with clear “not configured” UX, or `http://127.0.0.1:<port>` if truly local.
- Never ship with a default API key value.
- Add validation in `Settings::validate()` to fail fast on obviously invalid/missing endpoints.

### 5) Model downloads have no integrity verification

- `tauri-app/src-tauri/src/models.rs` downloads large binaries from GitHub/HuggingFace but does not verify checksums/signatures.

Recommendations:
- Pin known-good model hashes (SHA-256) and verify after download.
- Add timeouts to the blocking `reqwest` client used in downloads.
- Prefer HTTPS-only, and fail closed if TLS errors occur.

---

## P1 — Correctness & Behavior Gaps

### 1) “Remote-only transcription” is hard-coded, but local Whisper code and model UX still exist

Confirmed:
- Pipeline always uses remote server (`tauri-app/src-tauri/src/pipeline.rs`).
- Frontend settings treat whisper as remote-only (`tauri-app/src/hooks/useSettings.ts`).
- Yet there’s still local Whisper infrastructure:
  - `whisper-rs` dependency and `WhisperProvider` in `tauri-app/src-tauri/src/transcription.rs`
  - Whisper GGML model downloads and status (`tauri-app/src-tauri/src/models.rs`, `tauri-app/src-tauri/src/commands/models.rs`)
  - `check_model_status` checks local GGML model file even though remote mode is used.

Recommendations:
- Choose one clear direction and make the code consistent:
  - If remote-only: remove local model UI, local model checks, and dead code paths.
  - If hybrid/offline is a goal: implement the `whisper_mode` switch end-to-end in `pipeline.rs` and settings validation, and ensure model download UX matches.
- Update any “offline transcription” claims only after verifying the pipeline supports it.

### 2) Greeting detection ignores the configured “fast model”

Confirmed:
- The UI lets users select `fast_model` (settings drawer).
- But `LLMClient::check_greeting` hard-codes `"fast-model"` instead of using config (`tauri-app/src-tauri/src/llm_client.rs:724-737`).

Recommendation:
- Pass the configured fast model into greeting detection (either store it in `LLMClient` or pass as a parameter).

### 3) OAuth token exchange/refresh does not consistently handle HTTP error responses

Confirmed:
- In `tauri-app/src-tauri/src/medplum.rs`, `exchange_code` and `refresh_token` call `.send().await?.json().await?` directly (no `error_for_status` / status handling), unlike other methods that use `handle_response`.

Recommendation:
- Standardize HTTP response handling:
  - Use `error_for_status` + structured error mapping
  - Avoid logging error bodies that may include sensitive details

### 4) Medplum “add SOAP to encounter” uses placeholder IDs

Confirmed:
- `tauri-app/src-tauri/src/commands/medplum.rs:702-707` indicates it uses placeholders instead of real patient/encounter context.

Recommendation:
- Fetch encounter details to obtain the correct patient reference, then upload documents with correct FHIR linkage.

### 5) Pipeline transcription errors are swallowed

Observed:
- In `tauri-app/src-tauri/src/pipeline.rs`, transcription failures log errors but do not reliably propagate to UI via `PipelineMessage::Error`.

Recommendations:
- Track consecutive failures and emit a session error state after a threshold.
- Surface “Whisper server unreachable / auth failed” explicitly in the UI.

---

## P1 — Reliability & Performance Improvements

### 1) Avoid creating a Tokio runtime per transcription call

Confirmed:
- `WhisperServerClient::transcribe_blocking` creates a new runtime per call (`tauri-app/src-tauri/src/whisper_server.rs:325-333`).
- Listening mode also creates a runtime per analysis (`tauri-app/src-tauri/src/listening.rs:612-650`).

Recommendations:
- Create one runtime per thread and reuse it, or switch to `reqwest::blocking` in the synchronous pipeline thread.
- Measure the impact: runtime creation overhead can be significant with frequent utterances.

### 2) Thread lifecycle management (stop without join)

Observed:
- `reset_session` stops a pipeline without joining (`tauri-app/src-tauri/src/commands/session.rs`).
- `stop_listening` stops listening without joining (`tauri-app/src-tauri/src/commands/listening.rs`).

Recommendations:
- Join threads in a background task to avoid resource leaks and ensure clean shutdown.
- Add state to prevent starting multiple pipelines/listening threads concurrently.

### 3) Forced process exit bypasses cleanup and log flushing

Confirmed:
- On window close, the app calls `libc::_exit(0)` even after “graceful” shutdown (`tauri-app/src-tauri/src/lib.rs:312` and `tauri-app/src-tauri/src/lib.rs:317`).

Recommendations:
- Treat `_exit` as a last resort.
- Upgrade the `ort` crate / ONNX runtime integration to remove the crash-on-drop condition.
- Ensure the logging worker flushes before process exit (current `OnceLock<WorkerGuard>` design makes explicit flushing harder).

### 4) Atomic + permissioned writes for config/auth/logs/audio

Confirmed:
- `Config::save` writes directly with `std::fs::write` (`tauri-app/src-tauri/src/config.rs:479-489`).
- Medplum auth save similarly writes directly (`tauri-app/src-tauri/src/medplum.rs:101-113`).

Recommendations:
- Use atomic write pattern: write to temp → fsync → rename.
- Set strict permissions on sensitive files.

### 5) Large binary IPC payloads

Observed:
- History audio fetch returns `Vec<u8>` to the UI, then converts to `number[]` in TS (`tauri-app/src/components/AudioPlayer.tsx`).

Recommendations:
- Avoid sending large blobs over IPC as JSON arrays.
- Prefer:
  - writing the audio to a temp file and returning a file path, or
  - streaming/chunking, or
  - using a custom protocol/asset serving approach.

---

## P2 — Maintainability & DX

### 1) Duplicate ErrorBoundary implementations

Confirmed:
- `tauri-app/src/ErrorBoundary.tsx` and `tauri-app/src/components/ErrorBoundary.tsx` are separate implementations with different UX/styling.

Recommendation:
- Consolidate to one, export it consistently, and ensure styling is coherent.

### 2) CI “continue-on-error” on important quality gates

Observed:
- `.github/workflows/ci.yml` uses `continue-on-error: true` for clippy, coverage thresholds, and E2E.

Recommendation:
- If these are meant as real gates, remove `continue-on-error` or split into “required” vs “informational” jobs explicitly.

### 3) Remove/align dead configuration and doc drift

Recommendations:
- Align README claims with actual behavior (remote-only vs offline).
- Remove unused settings/commands or wire them back in.
- Consider a config schema version migration strategy (there’s a `schema_version` field but no migrations).

---

## Suggested Roadmap (Pragmatic “Next Level” Plan)

### Phase 0 (1–3 days): Safety hotfixes
- Remove/redact PHI in all logs and console output (listed P0 locations).
- Default `debug_storage_enabled` to `false` and gate debug storage behind explicit opt-in.
- Stop logging deep link URLs / OAuth codes anywhere.

### Phase 1 (1–2 weeks): Secure storage + reliable error handling
- Move tokens/API keys into OS secure storage.
- Add atomic writes and strict permissions for any remaining on-disk sensitive data.
- Propagate Whisper/LLM/Medplum connection failures to UI with actionable messages.
- Fix greeting detection to use configured `fast_model`.

### Phase 2 (2–6 weeks): Architecture + performance hardening
- Decide: remote-only vs hybrid/offline; then refactor pipeline + settings + UI accordingly.
- Reuse a single runtime / blocking client in pipeline and listening mode.
- Replace `_exit` with a real shutdown once ONNX teardown is stable.
- Improve Medplum linkage correctness (patient/encounter references for uploads).

### Phase 3 (6–12 weeks): Clinical readiness enhancements
- Add a data retention model (what is stored locally, for how long, and how to purge).
- Add audit logging that is explicitly PHI-safe (enforced by tests).
- Consider sandboxing/hardened runtime for macOS distribution (entitlements currently set `app-sandbox` false in `tauri-app/src-tauri/entitlements.plist`).

---

## Appendix — High-Signal File Map (for implementation work)

- Tauri startup + shutdown: `tauri-app/src-tauri/src/lib.rs`
- Session lifecycle (IPC): `tauri-app/src-tauri/src/commands/session.rs`
- Audio/transcription pipeline: `tauri-app/src-tauri/src/pipeline.rs`, `tauri-app/src-tauri/src/vad.rs`, `tauri-app/src-tauri/src/audio.rs`
- Listening mode: `tauri-app/src-tauri/src/listening.rs`, `tauri-app/src-tauri/src/commands/listening.rs`
- Whisper server client: `tauri-app/src-tauri/src/whisper_server.rs`
- LLM router client: `tauri-app/src-tauri/src/llm_client.rs`, `tauri-app/src-tauri/src/commands/ollama.rs`
- Medplum integration: `tauri-app/src-tauri/src/medplum.rs`, `tauri-app/src-tauri/src/commands/medplum.rs`
- Settings persistence: `tauri-app/src-tauri/src/config.rs`, `tauri-app/src-tauri/src/commands/settings.rs`
- Activity logging: `tauri-app/src-tauri/src/activity_log.rs`
- UI auth: `tauri-app/src/components/AuthProvider.tsx`
- UI session state: `tauri-app/src/hooks/useSessionState.ts`, `tauri-app/src/App.tsx`
