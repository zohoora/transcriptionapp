# Transcription App Code Review Findings (Code-First)

Date: 2026-02-06 (initial review), 2026-02-17 (latest update)
Scope: `tauri-app/src` and `tauri-app/src-tauri/src` (security intentionally deprioritized per request)

**Resolution Status (2026-02-17)**: Of the original 18 findings: 11 fixed, 5 deferred (1 mostly resolved), 2 open (#10, #16). Additionally, 16 new findings were fixed in a Feb 17 round (see update section below).

## How this review was performed
- Read implementation code directly (frontend + Rust backend + command wiring + archive + Medplum + continuous mode).
- Ignored documentation claims where behavior in code differed.
- Ran validation commands (as of 2026-02-17):
  - `pnpm -C tauri-app typecheck` (passes, 0 errors)
  - `pnpm -C tauri-app lint` (fails: 2 errors, 111 warnings)
  - `pnpm -C tauri-app test:run` (387 passed, 6 skipped)
  - `cargo test --lib` in `tauri-app/src-tauri` (421 passed, 0 failed)
  - `cargo check` (0 warnings)

---

## Findings (Prioritized)

### 1) [Critical] Auto-ended sessions are not finalized through the persistence path — FIXED
- Evidence:
  - Auto-end is detected and emitted in `tauri-app/src-tauri/src/commands/session.rs:248`.
  - On `PipelineMessage::Stopped`, code only marks complete and emits UI events (`tauri-app/src-tauri/src/commands/session.rs:262`).
  - Archive/debug save happens in `stop_session()` only (`tauri-app/src-tauri/src/commands/session.rs:385`, `tauri-app/src-tauri/src/commands/session.rs:422`).
  - Archive writes hardcode `auto_ended = false` (`tauri-app/src-tauri/src/commands/session.rs:391`, `tauri-app/src-tauri/src/commands/session.rs:430`).
- Impact:
  - Auto-ended sessions can be missing from local archive/debug output or carry incorrect metadata.
- Recommendation:
  - Create a shared `finalize_session(...)` function used by both manual stop and pipeline-stopped paths.
  - Persist auto-end reason in session/pipeline state and pass it into archive/debug writes.
  - Ensure finalization is idempotent.
- Tests to add:
  - Integration test: auto-end due to silence -> archive entry exists with `auto_ended=true` and reason.

### 2) [Critical] `medplum_add_soap_to_encounter` writes SOAP with incorrect patient reference — FIXED
- Evidence:
  - Uses encounter ID for all three IDs: `upload_soap_note(&encounter_fhir_id, &encounter_fhir_id, &encounter_fhir_id, ...)` in `tauri-app/src-tauri/src/commands/medplum.rs:707`.
  - SOAP upload sets `subject` as `Patient/{patient_id}` in `tauri-app/src-tauri/src/medplum.rs:911`.
- Impact:
  - SOAP DocumentReference can point to a non-patient resource ID in `subject`, corrupting clinical linkage.
- Recommendation:
  - Fetch encounter first, extract `subject.reference` patient ID, pass actual patient ID into `upload_soap_note`.
  - Fail loudly if patient reference is missing.
- Tests to add:
  - Unit test that add-soap uses patient ID from encounter subject.

### 3) [Critical] Quick sync paths mark encounters synced but never complete them — DEFERRED (intentional behavior)
- Evidence:
  - `medplum_quick_sync` sets `sync_status.encounter_synced = true` without calling `complete_encounter` (`tauri-app/src-tauri/src/commands/medplum.rs:662`).
  - `medplum_multi_patient_quick_sync` similarly never completes encounters (`tauri-app/src-tauri/src/commands/medplum.rs:768`).
- Impact:
  - Encounters remain `in-progress` indefinitely while UI reports successful sync.
- Recommendation:
  - After successful uploads, call `complete_encounter` for each created encounter.
  - Only set encounter synced after completion succeeds.
- Tests to add:
  - Verify quick-sync and multi-patient quick-sync transition encounter status to `finished`.

### 4) [Critical] Rust/TypeScript contract mismatch in multi-patient sync payload — FIXED
- Evidence:
  - Rust returns snake_case fields (`patient_label`, `encounter_fhir_id`) in `tauri-app/src-tauri/src/commands/medplum.rs:742`.
  - TS expects camelCase (`patientLabel`, `encounterFhirId`) in `tauri-app/src/types/index.ts:527`.
  - Frontend reads camelCase in `tauri-app/src/hooks/useMedplumSync.ts:143`.
- Impact:
  - Runtime values are undefined in frontend for patient/encounter IDs after multi-patient sync.
- Recommendation:
  - Add serde rename policy on Rust structs (`#[serde(rename_all = "camelCase")]`) or align TS to snake_case everywhere.
  - Add a strict contract test for command payload shape.

### 5) [High] Continuous mode and auto-detection/listening can run at the same time — FIXED
- Evidence:
  - App passes a continuous-mode guard to `useAutoDetection` (`tauri-app/src/App.tsx:198`), but hook ignores it (`tauri-app/src/hooks/useAutoDetection.ts:43`).
  - Auto-listening effect does not gate on continuous mode (`tauri-app/src/App.tsx:410`).
- Impact:
  - Competing microphone pipelines, flaky start behavior, and inconsistent UX.
- Recommendation:
  - Use the `autoStartEnabled` parameter in `useAutoDetection` to actively stop/start listener.
  - In `App.tsx`, explicitly disable listening while continuous mode is enabled.

### 6) [High] Switching charting mode can leave continuous recording running in background — FIXED
- Evidence:
  - Continuous UI controls are only rendered when `isContinuousMode` is true (`tauri-app/src/App.tsx:708`).
  - No effect exists to stop continuous backend when switching mode to `session`.
  - Charting mode is switchable in settings (`tauri-app/src/components/SettingsDrawer.tsx:140`).
- Impact:
  - Hidden background recording with no visible stop control.
- Recommendation:
  - On charting mode change away from continuous, auto-call `stop_continuous_mode` and await confirmation.
  - Block mode switch while continuous is active unless stop succeeds.

### 7) [High] `reset_session` can still block despite code comment claiming non-blocking — FIXED
- Evidence:
  - Comment says no join in reset path (`tauri-app/src-tauri/src/commands/session.rs:519`).
  - But dropping `PipelineHandle` joins thread in `Drop` (`tauri-app/src-tauri/src/pipeline.rs:188`).
- Impact:
  - UI/main-thread stalls are possible during reset.
- Recommendation:
  - Refactor `PipelineHandle` so `Drop` does not join on UI-sensitive paths, or move handle disposal to explicit background join worker.

### 8) [High] UTF-8 unsafe slicing can panic in continuous transcript preview — FIXED
- Evidence:
  - Byte slicing `&text[text.len() - 500..]` in `tauri-app/src-tauri/src/continuous_mode.rs:495`.
- Impact:
  - Panic on non-ASCII boundary splits.
- Recommendation:
  - Use `char_indices()` boundary-safe truncation or keep a rolling preview buffer by chars/graphemes.

### 9) [High] Local history audio playback path is broken — FIXED (backend only)
- Evidence:
  - Local path is wrapped as `file://...` in `tauri-app/src/components/HistoryWindow.tsx:754`.
  - `AudioPlayer` always assumes Medplum Binary and invokes `medplum_get_audio_data` (`tauri-app/src/components/AudioPlayer.tsx:46`).
- Impact:
  - Local archived audio likely fails to load/play.
- Recommendation:
  - Support two modes in `AudioPlayer`: local file URL passthrough vs Medplum binary fetch.
  - Use explicit prop type (`source: 'local' | 'medplum'`).

### 10) [High] Archive metadata timestamps are logically incorrect (and segment count is never populated) — OPEN
- Evidence:
  - `started_at` initialized to `Utc::now()` in `ArchiveMetadata::new` (`tauri-app/src-tauri/src/local_archive.rs:80`).
  - `save_session` runs at stop and sets `ended_at` to now (`tauri-app/src-tauri/src/local_archive.rs:138`), so start/end are effectively stop-time-derived.
  - `segment_count` initialized as 0 and not updated in `save_session` (`tauri-app/src-tauri/src/local_archive.rs:83`, `tauri-app/src-tauri/src/local_archive.rs:137`).
- Impact:
  - History timing can be misleading (especially around day boundaries); segment metrics are always wrong.
- Recommendation:
  - Persist true session start timestamp from session state.
  - Pass and store segment count when finalizing.

### 11) [High] Continuous mode lifecycle can report active even when startup failed — FIXED
- Evidence:
  - Backend emits `started` before pipeline startup succeeds (`tauri-app/src-tauri/src/continuous_mode.rs:367`).
  - Hook sets `isActive=true` immediately after invoke (`tauri-app/src/hooks/useContinuousMode.ts:149`).
  - On error event, hook sets only `error` and not `isActive=false` (`tauri-app/src/hooks/useContinuousMode.ts:74`).
- Impact:
  - False active UI state with stale controls.
- Recommendation:
  - Emit `started` only after pipeline start success.
  - In hook, set `isActive=false` on `error` and on failed status poll.

### 12) [High] Continuous mode records full-day WAV files that are never surfaced or cleaned — DEFERRED
- Evidence:
  - Continuous pipeline always configures `audio_output_path` (`tauri-app/src-tauri/src/continuous_mode.rs:389`).
  - No archive linkage or cleanup code references those files.
- Impact:
  - Silent storage growth and orphaned recordings.
- Recommendation:
  - Decide product behavior: keep-and-link, rotate, or disable full-day raw recording.
  - Implement retention policy and history linkage if kept.

### 13) [Medium] Session and continuous mode share `transcript_update` event channel (cross-talk) — FIXED
- Evidence:
  - Session hook subscribes globally (`tauri-app/src/hooks/useSessionState.ts:112`).
  - Continuous mode emits transcript preview using same event (`tauri-app/src-tauri/src/continuous_mode.rs:499`).
- Impact:
  - Session transcript state can be polluted by continuous preview payloads.
- Recommendation:
  - Split event names (`session_transcript_update` vs `continuous_transcript_preview`) and gate listeners by mode.

### 14) [Medium] Medplum history/document lookup has truncation and scaling issues — DEFERRED
- Evidence:
  - Encounter history query hard-limits `_count=100` with no pagination (`tauri-app/src-tauri/src/medplum.rs:1062`).
  - Document/media lookups hard-limit `_count=200` with no pagination (`tauri-app/src-tauri/src/medplum.rs:1220`, `tauri-app/src-tauri/src/medplum.rs:1269`).
  - Patient names are fetched per encounter (N+1) (`tauri-app/src-tauri/src/medplum.rs:1140`).
- Impact:
  - Incomplete history flags and degraded performance for larger datasets.
- Recommendation:
  - Implement pagination (`link.next` traversal).
  - Reduce N+1 by batching or using `subject.display` fallback/caching.

### 15) [Medium] `useAutoDetection` callback contract mismatches async usage — FIXED
- Evidence:
  - Callback types are `() => void` in `tauri-app/src/hooks/useAutoDetection.ts:17`.
  - App passes async handlers (`tauri-app/src/App.tsx:168`, `tauri-app/src/App.tsx:179`).
  - Hook invokes them without awaiting/catching (`tauri-app/src/hooks/useAutoDetection.ts:135`, `tauri-app/src/hooks/useAutoDetection.ts:154`).
- Impact:
  - Unhandled promise rejection risk and hidden start/reset failures.
- Recommendation:
  - Type callbacks as `() => Promise<void> | void` and handle with `await` + `try/catch`.

### 16) [Medium] Lint gate is currently red; codebase has high warning volume — OPEN
- Evidence:
  - `pnpm -C tauri-app lint` reports 2 errors and 111 warnings (as of 2026-02-17).
  - Hard errors are `no-useless-escape` in markdown parser regex: `tauri-app/src/components/ClinicalChat.tsx:44`.
  - Warnings are predominantly `no-console` and a few hook dependency warnings.
- Impact:
  - Noise masks meaningful regressions; CI quality signal degraded.
- Recommendation:
  - Fix the 2 hard errors immediately.
  - Triage hook dependency warnings; remove dead code paths; reduce no-console warnings for production.

### 17) [Medium] Test stability problems reduce trust in CI outcomes — DEFERRED (environment-specific)
- Evidence:
  - Frontend `vitest run` exits non-zero due worker OOM (Node heap ~4GB) despite most tests passing.
  - Rust unit tests fail in this environment due `system-configuration` NULL object panic in client constructor tests:
    - `tauri-app/src-tauri/src/llm_client.rs:2054`
    - `tauri-app/src-tauri/src/whisper_server.rs:417`
    - `tauri-app/src-tauri/src/medplum.rs:1541`
  - Audio default-device test is environment-sensitive and currently fails assertion (`tauri-app/src-tauri/src/audio.rs:609`).
- Impact:
  - CI can be red for infra reasons unrelated to product regressions.
- Recommendation:
  - Add headless/CI-safe test guards or dependency injection for reqwest client creation.
  - Tune Vitest worker/memory settings and isolate noisy suites.

### 18) [Low] `App.tsx` is an orchestration hotspot (~840 lines) — DEFERRED (low priority)
- Evidence:
  - `tauri-app/src/App.tsx` owns auth, settings, sync, lifecycle, continuous mode, chat, media, and UI routing.
- Impact:
  - High change risk and weak local reasoning; regression probability increases.
- Recommendation:
  - Extract mode controllers and command orchestration into composable hooks/modules.
  - Keep `App.tsx` as assembly + routing only.

---

## Additional implementation ideas (non-blocking but high leverage)
1. Add a typed IPC contract layer (single source for Rust serde + TS types) and contract tests for command payload shape.
2. Create a unified session finalization service handling: status transition, transcript snapshot, archive/debug persistence, Medplum enqueue, and telemetry.
3. Separate event namespaces by domain (`session.*`, `continuous.*`, `listening.*`) to prevent accidental cross-consumption.
4. Add retention and observability around recordings/archive growth (file count, size, prune policy).

---

## Suggested implementation order
1. Fix data integrity issues first: findings #1, #2, #3, #4.
2. Fix runtime correctness/lifecycle collisions: findings #5, #6, #7, #8, #11.
3. Fix user-visible functionality: findings #9, #10, #12, #13.
4. Address maintainability and reliability: findings #14-#18.

---

## Open questions / assumptions to confirm
- ~~Should quick-sync encounters be auto-completed, or intentionally left in-progress for later edits?~~ **Resolved**: Intentionally left in-progress (Finding #3 deferred).
- Should continuous mode keep a full-day raw audio file, or only per-encounter artifacts? (Finding #12 deferred)
- Should local archive remain the primary history source even when Medplum is enabled?

---

## Resolution Summary (2026-02-06, initial review)

| Finding | Status | Resolution |
|---------|--------|------------|
| #1 Auto-ended sessions not archived | **FIXED** | Added archive + debug save in `Stopped` handler with `auto_end_triggered` flag |
| #2 Wrong patient ID in SOAP upload | **FIXED** | Fetch encounter to extract `subject.reference` patient ID before upload |
| #3 Quick sync encounters left in-progress | **DEFERRED** | Intentional behavior — encounters left open for later edits |
| #4 Serde snake_case/camelCase mismatch | **FIXED** | Added `#[serde(rename_all = "camelCase")]` to `PatientSyncInfo`, `MultiPatientSyncResult` |
| #5 Continuous + auto-detection conflict | **FIXED** | Added `!isContinuousMode` guard to listening effect in App.tsx |
| #6 Charting mode switch while active | **FIXED** | Block switch from continuous to session while recording is active |
| #7 reset_session blocking | **FIXED** | Pipeline handle join moved to background `std::thread::spawn` |
| #8 UTF-8 panic in transcript preview | **FIXED** | Using `ceil_char_boundary()` for safe string slicing |
| #9 Local audio playback broken | **FIXED** (backend) | Added `read_local_audio_file` command with path traversal validation |
| #10 Archive timestamps incorrect | **OPEN** | Needs `started_at` and `segment_count` passed to `save_session` |
| #11 Continuous mode false active state | **FIXED** | Emit `started` only after pipeline succeeds; `isActive=false` on error |
| #12 Orphaned WAV files | **DEFERRED** | Leave for now; needs product decision on retention policy |
| #13 Event cross-talk | **FIXED** | Renamed to `continuous_transcript_preview` (separate from `transcript_update`) |
| #14 Medplum pagination | **DEFERRED** | Not urgent for current dataset sizes |
| #15 Async callback type mismatch | **FIXED** | Types changed to `void | Promise<void>`, wrapped with `Promise.resolve().catch()` |
| #16 Lint errors + warnings | **OPEN** | 2 `no-useless-escape` errors + 111 warnings remain (ClinicalChat.tsx:44) |
| #17 Test stability | **DEFERRED** | Environment-specific (Node OOM, system-configuration NULL) — mostly resolved, see Feb 17 update |
| #18 App.tsx size | **DEFERRED** | Low priority refactor (~921 lines as of Feb 17) |

---

## Feb 17, 2026 Update — Additional Fixes (Commit 87179cf)

A second round of fixes addressed 16 additional findings across backend and frontend. Test/lint status after this round:

- `cargo test --lib`: **421 passed**, 0 failed
- `pnpm test:run`: **387 passed**, 6 skipped
- `npx tsc --noEmit`: **0 errors**
- `cargo check`: **0 warnings**

### Backend Fixes (Rust)

| ID | Finding | Resolution |
|----|---------|------------|
| C1 | Path traversal in `local_archive.rs` | Added `validate_session_id()` — rejects path separators and special directory references |
| C2 | Encounter notes cleared before SOAP generation | Clone encounter notes before clearing the buffer, so SOAP receives the full transcript |
| I1 | Duplicate archival race in continuous mode | Added state guard in `Stopped` handler to prevent double-archive |
| I2 | Pipeline `Drop` blocks Tokio runtime | Join moved to `spawn_blocking` to avoid blocking the async runtime |
| I3 | Poisoned lock silent drops across codebase | Added `warn!` logging at 22 sites where poisoned `Mutex`/`RwLock` are recovered |
| I4 | FHIR ID validation missing | Added `validate_fhir_id()` in `medplum.rs` — rejects IDs with path traversal characters |
| I5 | Error body leakage in HTTP responses | Truncate error body strings to 200 chars before including in error messages |
| I6 | Token refresh TOCTOU race | Implemented double-check locking pattern for OAuth token refresh |
| I7 | Settings update skips validation | `clamp_values()` called after every settings update to enforce safe ranges |

### Frontend Fixes (TypeScript/React)

| Finding | Resolution |
|---------|------------|
| PendingSettings deduplication | Eliminated duplicate pending-settings tracking in settings hooks |
| Audio quality utility extraction | Extracted audio quality helper logic into shared utility functions |
| Patient aggregation utility | Extracted patient speaker aggregation into `aggregatePatientSpeakers()` in `utils.ts` |
| `useAutoDetection` callback refs | Fixed callback reference stability to prevent unnecessary re-renders |
| Redundant `setIsActive` removal | Removed redundant active-state setting in continuous mode hook |
| Dead code cleanup | Removed unused code paths identified during review |
| `useMemo` for `hasUnsavedChanges` | Memoized computed value to prevent unnecessary recalculations |
| Test mocks synced with real types | Updated test mocks to match current TypeScript interface definitions |

### Impact on Original Findings

- **Finding #7** (reset_session blocking): Further hardened — Pipeline `Drop` now uses `spawn_blocking` (I2)
- **Finding #10** (archive timestamps): Still **OPEN** — `started_at` and `segment_count` remain unresolved
- **Finding #16** (lint): Still **OPEN** — 2 errors (`no-useless-escape` in ClinicalChat.tsx) + 111 warnings
- **Finding #17** (test stability): **Mostly resolved** — Rust tests now 421 passed / 0 failed; frontend 387 passed / 6 skipped
- **Finding #18** (App.tsx size): Unchanged at ~921 lines (was ~840 at initial review)
