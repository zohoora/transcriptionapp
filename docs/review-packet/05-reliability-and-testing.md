# Reliability And Testing

## Test snapshot
I ran the current automated suites from the repo state on 2026-03-08.

### Frontend
Command:
- `pnpm test:run -- --reporter=dot`

Result:
- `21` test files passed
- `433` tests passed
- `6` tests skipped
- total duration about `5.25s`

### Backend
Command:
- `cargo test --quiet`

Result:
- `577` tests passed
- `14` tests failed
- `32` tests ignored
- total duration about `40.45s`

## What the frontend suite says
The frontend suite is green, but it is noisy in ways that matter.

### 1. There are many expected-error logs in passing tests
Examples surfaced during the run:
- device load errors
- settings load errors
- connection failures
- Medplum sync failures

This is not automatically bad, but it makes failure output harder to trust.

### 2. Several components leak async updates outside `act(...)`
Most obvious offenders in the test output:
- `src/components/SpeakerEnrollment.tsx:28`
- `src/components/HistoryWindow.tsx:55`
- `src/components/AuthProvider.tsx:51`
- some hook tests around `useSoapNote`

Interpretation:
- The UI is doing legitimate async work
- The test harness is not always controlling it tightly
- Regressions could get masked by warning fatigue

### 3. Contract tests are useful but far too narrow
`src/contracts.test.ts:1` validates only a small subset of the real IPC surface.

Examples of drift risk:
- The test `Settings` shape in `src/contracts.test.ts:38` only includes a handful of fields.
- The actual `Settings` interface in `src/types/index.ts:53` is much larger and includes continuous mode, image generation, sensor, and shadow-mode fields.

Recommendation:
- Replace hand-maintained contract spot checks with generated contracts or a schema-driven test.

## What the backend suite says
The backend failures are revealing. Most are not random logic failures; they expose environmental coupling.

### Failure class 1: local archive integration tests assume writable home-directory behavior
Failing tests:
- `local_archive::tests::test_delete_session_integration`
- `test_get_transcript_lines_integration`
- `test_merge_sessions_integration`
- `test_renumber_encounters_integration`
- `test_split_session_edge_cases_integration`
- `test_split_session_integration`
- `test_update_patient_name_integration`

Observed failure:
- `PermissionDenied` from `src/local_archive.rs` integration paths

Relevant implementation:
- archive root is based on home directory: `src-tauri/src/local_archive.rs:46`
- archive operations are file-system heavy throughout the module

Interpretation:
- Integration tests are not hermetic enough
- Archive code needs a testable storage root injection mechanism

### Failure class 2: HTTP client construction can panic in this environment
Failing tests:
- `llm_client::tests::test_llm_client_new`
- `llm_client::tests::test_llm_client_new_trailing_slash`
- `medplum::tests::test_client_creation`
- `gemini_client::tests::test_new_valid_api_key`
- `whisper_server::tests::test_whisper_server_client_new`

Observed failure:
- panic inside `system-configuration` with `Attempted to create a NULL object`

Interpretation:
- Networking client setup is still coupled to specific macOS runtime facilities
- Client-construction tests should not rely on ambient platform state

### Failure class 3: audio tests assume device-name behavior that is not stable everywhere
Failing tests:
- `audio::tests::test_get_device_default`
- `audio::tests::test_get_device_explicit_default`

Observed failure:
- assertion that `result.unwrap().name().is_ok()`

Interpretation:
- Tests assume more about the audio backend than the environment guarantees

## Runtime reliability issues visible from code

### 1. Forced process exit on shutdown
`src-tauri/src/lib.rs:463` documents the ONNX runtime cleanup problem explicitly.
`src-tauri/src/lib.rs:474` and `:479` both terminate via `_exit(0)`.

This means:
- graceful resource cleanup is not actually guaranteed
- shutdown-related bugs may be hidden instead of fixed

### 2. Screenshot count likely over-reports
The screenshot subsystem stores both full and thumb paths:
- full path pushed at `src-tauri/src/screenshot.rs:175`
- thumb path pushed at `src-tauri/src/screenshot.rs:189`

But status reports total vector length:
- `src-tauri/src/screenshot.rs:148`
- `src-tauri/src/commands/screenshot.rs:84`

And the UI uses that value directly in review mode:
- `src/App.tsx:505`
- `src/App.tsx:718`

This is a small bug, but it is a good example of why contract-level tests should cover behavior, not just types.

### 3. History source semantics are unstable
`HistoryWindow` uses `debug_storage_enabled` to choose local vs Medplum history:
- `src/components/HistoryWindow.tsx:71`
- `src/components/HistoryWindow.tsx:116`

But local archive is explicitly production-ready storage:
- `src-tauri/src/local_archive.rs:3`

This is a product reliability problem because it can make users think history disappeared when a debug-oriented flag changes.

### 4. Settings persistence semantics are inconsistent
The backend validation path is sound:
- `src-tauri/src/config.rs:431`
- `src-tauri/src/commands/settings.rs:12`

But the frontend persists settings from multiple non-save paths:
- `src/App.tsx:207`
- `src/hooks/useConnectionTests.ts:107`
- `src/hooks/useOllamaConnection.ts:84`
- `src/hooks/useSoapNote.ts:91`

This creates state drift and is a reliability issue even if nothing crashes.

## Testing priorities

### Highest priority
1. Make local archive integration tests hermetic.
- Inject archive root instead of relying on actual home-directory behavior.
- Separate storage-path tests from archive business-logic tests.

2. Make client-construction tests platform-agnostic.
- Avoid ambient macOS system configuration dependencies in constructor tests.
- Use builder abstraction or mocked client creation where needed.

3. Add behavioral contract tests for critical cross-layer flows.
- settings save/test semantics
- history source selection
- screenshot count semantics
- continuous mode event/state shapes

### Medium priority
1. Fix `act(...)` leaks in frontend tests.
2. Reduce expected-error console noise in successful test runs.
3. Add end-to-end fixtures for continuous encounter splitting and merge-back.

### Long-term priority
1. Build replayable transcript fixtures for AI task evaluation.
2. Add a regression suite specifically for continuous mode decisions.
3. Add shutdown/lifecycle soak testing once the ONNX exit path is improved.

## Reliability bottom line
The app is already testable and much of it is covered. The next maturity step is not "write more tests everywhere". It is:
- make the existing tests hermetic
- make cross-layer contracts explicit
- stop tolerating noisy warnings as normal
- put the hardest product behavior, especially continuous charting, under targeted regression coverage
