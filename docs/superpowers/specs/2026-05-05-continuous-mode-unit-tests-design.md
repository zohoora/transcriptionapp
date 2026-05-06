# Unit Tests for `audio_processing`, `continuous_mode_splitter`, `continuous_mode_flush_on_stop`

**Date:** 2026-05-05
**Branch:** `chore/regression-ci-ratchet`
**Status:** Approved (verbal)

## Goal

Add inline unit-test coverage to three modules that currently have effectively none:

| Module | Lines | Existing tests |
|---|---|---|
| `src/audio_processing.rs` | 140 | 0 |
| `src/continuous_mode_splitter.rs` | 411 | 0 |
| `src/continuous_mode_flush_on_stop.rs` | 885 | 1 (constant-invariant guard) |

These three sit on the hot path between transcript capture and durable session archive, and changes to any of them have produced clinic incidents (Apr 16 Grantham flush mis-numbering, Apr 21 Spencer missing `replay_bundle.json`, etc.). Today they are covered only at the higher integration levels (`harness_per_encounter`, `labeled_regression_cli`, the new PR-side regression ratchet) — not at the unit level where regressions can be pinned to a single function.

## Non-goals

- Refactoring `LLMClient` → `Arc<dyn LlmBackend>` to enable LLM-using paths in `flush_on_stop`. Cascading change across `encounter_pipeline` helpers; out of scope.
- Wiremock-based HTTP mocking for the LLM-using branches. Couples test assertions to LLM prompt/response wire format; the regression CLIs already cover that surface against real archived data.
- Coverage of `continuous_mode.rs` itself or sibling modules (`post_split`, `merge_back`, `forward_merge`, `trigger_wait`).

## Approach

### `audio_processing.rs` — full pure-function coverage

Inline `#[cfg(test)] mod tests` at end of file. ~12 tests covering:

- `is_supported_format`: lowercased + uppercased extensions for every format in `SUPPORTED_EXTENSIONS`; `.txt` rejected; missing extension rejected; empty path rejected.
- `split_transcript_into_encounters`: V1 contract — always returns a single-element `Vec<String>`. Tested with empty / short / long / whitespace-only transcripts.
- `read_wav_samples`: int16 mono normalized to `f32` in `[-1, 1]` (full-scale `i16::MAX` → ~`0.99997`, `i16::MIN` → `-1.0`); float32 mono returned unchanged (identity); stereo input errors with `"Expected mono WAV"`; nonexistent path errors.

WAV fixtures built in-test via `hound::WavWriter` into `tempfile::tempdir()`. **Not tested:** `transcode_to_wav` and `check_ffmpeg_available` — they exec a subprocess and the `OnceLock` cache makes the test order-dependent across runs. Those are exercised by the manual-upload e2e and `process_mobile` integration paths.

### `continuous_mode_splitter.rs` — integration-style via `RecordingRunContext`

Inline `#[cfg(test)] mod tests` + private `mod test_helpers` for shared builders. Test infrastructure:

- `RecordingRunContext::new(...)` from `harness/recording_run_context.rs` for event capture + clock virtualization.
- `tempfile::tempdir()` + `TRANSCRIPTIONAPP_ARCHIVE_DIR` env override for filesystem isolation (already an established pattern in `local_archive.rs` tests).
- `ServerSyncContext::empty()` for a no-op sync context.
- `ContinuousModeHandle::new()` for transcript buffer + name tracker + counters.
- `PipelineLogger::new()`, `SegmentLogger::new()`, `ReplayBundleBuilder::new(serde_json::json!({}))` for logger deps.

Test cases (~5):

1. **Happy path** — buffer with N segments produces a `SplitContext` with correct `encounter_word_count`, `encounter_segment_count`, `encounter_duration_ms` (computed from first/last segment timestamps).
2. **Metadata enrichment** — written `metadata.json` has `charting_mode="continuous"`, `encounter_number = loop_state.encounter_number`, `detection_method` matches the `SplitterCall::detection_method`, `patient_name` = name tracker majority.
3. **Name tracker reset** — votes / streak / DOB cleared after split; the snapshot returned in `SplitContext.tracker_snapshot` reflects pre-reset state.
4. **Counters** — `encounters_detected` increments by 1; `recent_encounters` truncates to 3 after 4+ splits in a row.
5. **Event emission** — captured events on the `RecordingRunContext` contain `EncounterDetected { session_id, word_count, patient_name }` with the values from the returned `SplitContext`.

### `continuous_mode_flush_on_stop.rs` — `llm_client = None` paths

Inline `#[cfg(test)] mod tests`. Same harness as splitter, with:

- `FlushOnStopHandles` populated with `PipelineHandle::for_testing()` and `tokio::spawn(async {})` for the consumer/detector/screenshot/shadow/sensor-monitor task slots — all exit immediately when polled.
- `FlushOnStopDeps` with `llm_client: None`, no-op sync_ctx, fresh loggers/bundles/templates.
- `RecordingRunContext::raw_tauri_app()` returns `None`, so `recover_orphaned_soap` is gated off automatically.

Test cases (~5):

1. **Empty buffer** — no flush session is created (no entries under archive root), `performance_summary.json` is written, captured events end with `Stopped`, and `handle.state` ends as `ContinuousState::Idle`.
2. **<100-word buffer** — `if word_count > 100` is false, so nothing in the buffer block runs; same observables as test 1 (no flush session, Stopped emitted, state Idle).
3. **>100-word buffer + `llm_client = None`** — session archived under `archive/YYYY/MM/DD/{uuid}/`, metadata.json has `detection_method="flush"` + `encounter_number = prev.encounter_number + 1`, no SOAP file, `replay_bundle.json` finalized.
4. **Encounter-number derivation** *(user-contributed fixture)* — pre-seed the archive with 1+ prior session(s) that have `encounter_number` set, then run flush; assert the new flush session's `encounter_number` is `prev.encounter_number + 1`. Designed to fail if someone reverts to `sessions.len()` (the Apr 16 Grantham bug).
5. **Negative-gap pair scan** — pre-seed two adjacent sessions with the negative-gap pattern (next started <30s after prev ended, prev tail <2500 words); assert `NegativeGapPairsFound` is captured.

The existing multi-patient threshold guard test stays as-is.

## Filesystem isolation

All three modules' filesystem-touching tests set `TRANSCRIPTIONAPP_ARCHIVE_DIR` to `tempdir.path()` before any `local_archive::*` call. Tempdir is held for the duration of the test and dropped at end; `std::env::set_var` / `remove_var` is wrapped in a guard pattern (matches `local_archive.rs:2138-2143`).

## Verification

```bash
cd tauri-app/src-tauri
cargo test --lib audio_processing
cargo test --lib continuous_mode_splitter
cargo test --lib continuous_mode_flush_on_stop
cargo test --lib  # full suite — ensure no regressions
```

Acceptance:
- All new tests pass under `cargo test --lib` (no `--ignored`).
- Full suite runs in <10s additional wall time.
- No writes to `~/.transcriptionapp/`.

## Risks

| Risk | Mitigation |
|---|---|
| `TRANSCRIPTIONAPP_ARCHIVE_DIR` env interaction across parallel tests | Tests run serially within a thread by default; cargo's per-test process isolation isn't relied on. The env var is read on every `local_archive` call so within-test consistency is fine. |
| `ServerSyncContext::empty()` may still attempt server calls | Audited: `empty()` returns a no-op context — `sync_session()` etc. become no-ops when client is unconfigured. |
| `PipelineHandle::for_testing()` may not match production lifecycle | Already used elsewhere in the harness; if `flush_on_stop` calls a method not implemented in test mode we'll add it. |

## Out of scope follow-ups

- LLM-using path coverage in flush_on_stop — covered by harness + labeled_regression_cli; revisit only if a specific class of regression slips through.
- Tests for `transcode_to_wav` / ffmpeg path resolution — needs a process-isolation harness or a dependency-injection seam for the ffmpeg binary; not worth the scaffold.
