# Transcription App Review

## Findings (ordered by severity)

### Critical
- PHI leakage in logs: transcript text is logged at info level, but activity logging explicitly forbids transcript content. Evidence: `tauri-app/src-tauri/src/activity_log.rs:1` and `tauri-app/src-tauri/src/commands.rs:284` plus `tauri-app/src-tauri/src/pipeline.rs:760`. Suggested fix: remove or redact segment text in all `info!/debug!` logs; keep only IDs, timings, or word counts.

### High
- Whisper model downloads read the entire response into memory (`response.bytes()`), which will OOM for medium/large models (1.5GB+). Evidence: `tauri-app/src-tauri/src/models.rs:168`. Suggested fix: stream the response to disk in chunks (reqwest streaming) and optionally surface progress.
- Download button cannot download Whisper models: `download_whisper_model` requires `model_name`, but the UI invokes the command without args. Evidence: `tauri-app/src/App.tsx:153` and `tauri-app/src-tauri/src/commands.rs:519`. Suggested fix: pass `{ modelName }` (or `{ model_name }` depending on invoke binding) when calling `download_whisper_model`.
- Tail audio can be dropped on stop: when `stop_flag` is set, the pipeline flushes only the current VAD buffer and does not drain remaining ring buffer or staging buffer before final transcription. Evidence: `tauri-app/src-tauri/src/pipeline.rs:422` and `transcribe-cli/src/audio/processor.rs:109`. Suggested fix: on stop, drain ring buffer into resampler/staging buffer, process remaining VAD chunks, then force-flush.

### Medium
- Session start/stop logs use different random IDs, so a single session cannot be correlated in logs. Evidence: `tauri-app/src-tauri/src/commands.rs:252` and `tauri-app/src-tauri/src/commands.rs:351`. Suggested fix: create and store a session_id in `SessionManager`, reuse for all lifecycle events.
- Downmixing uses only channel 0; stereo inputs with signal in the right channel will be silent or reduced. Evidence: `tauri-app/src-tauri/src/audio.rs:327` and `transcribe-cli/src/audio/capture.rs:96`. Suggested fix: average channels (or RMS mix) instead of taking the first channel.
- Init path can leave checklist in a perpetual loading state if `run_checklist` fails, because `setChecklistRunning(false)` is only set on the happy path. Evidence: `tauri-app/src/App.tsx:182` and `tauri-app/src/App.tsx:231`. Suggested fix: move `setChecklistRunning(false)` into a `finally` block inside `init()`.

### Low
- README claims “Fully Offline,” but the UI performs network checks on startup (Ollama + Medplum). Evidence: `README.md:16` and `tauri-app/src/App.tsx:209`. Suggested fix: gate these checks behind explicit user actions or a setting, or update the README to reflect optional network features.
- Audio overrun/dropout detection exists but is never surfaced in UI or biomarker analysis. Evidence: `tauri-app/src-tauri/src/audio.rs:313` and `tauri-app/src-tauri/src/biomarkers/thread.rs:97` (no caller). Suggested fix: plumb overflow counts to `AudioQualityAnalyzer` or emit a warning event.

## Questions / Assumptions
- Is it acceptable to require zero transcript content in *all* logs (console + file), or only the activity log file? Current tracing setup writes all logs to file.
- Should “offline-only” be enforced by default, with Ollama/Medplum behind a toggle, or is the README outdated?

## Suggested Tests
- Stop-with-pending-buffer regression test to ensure the final utterance includes tail audio when `stop` is called mid-speech.
- Download-Whisper-model UI test to ensure the correct argument is passed and errors are surfaced.
- Log scrub test (unit or integration) to assert that transcript text never appears in log output.
