# ADR-0022: Manual Audio Upload

## Status

Accepted (Apr 2026)

## Context

Physicians occasionally need to process audio that wasn't captured in real-time through the app:

- Recordings made on a phone during a house call or covering shift.
- Recordings from prior visits that were never transcribed.
- Audio exported from a dictation device before the app existed.

The mobile-app-v1 design (see mobile app spec, `ios/`) already runs this exact pipeline on the server side for iOS recordings: ffmpeg transcode → STT batch → encounter detection → SOAP. We wanted the same capability on the desktop, without duplicating the pipeline code.

## Decision

Extract the shared pipeline into an `audio_processing` library module and call it from both `process_mobile` (CLI, iOS-driven) and a new `commands::audio_upload` Tauri command (desktop UI).

### Shared module: `audio_processing.rs`

| Function | Purpose |
|----------|---------|
| `transcode_to_wav()` | Invoke `ffmpeg` to produce 16 kHz mono PCM from any supported input format. Verifies ffmpeg is on PATH first. |
| `read_wav_samples()` | Load the transcoded WAV into a `Vec<f32>` for passing to STT. |
| `split_transcript_into_encounters()` | Apply `evaluate_detection()` over the full transcript to chop it into encounter segments using the same LLM loop the live pipeline uses. |

Both CLI and command call exactly these helpers — **zero algorithm divergence** with the live continuous-mode path.

### Supported formats

Any format `ffmpeg` can decode: mp3, wav, m4a, aac, flac, ogg, wma, webm. The UI file picker advertises these extensions.

### UI

- **Trigger points**: "Upload Recording" link in both ReadyMode (session-mode home) and ContinuousMode (top of the dashboard).
- **Modal**: `AudioUploadModal.tsx` — file picker + date picker. Date defaults to today; user can pick a prior date so the uploaded session lands in the correct day's archive.
- **Progress**: UI subscribes to the `audio_upload_progress` Tauri event and shows stage transitions: `transcoding → transcribing → detecting → generating_soap → complete | failed`.
- **Charting mode marker**: Uploaded sessions are stored with `charting_mode = "upload"` so they're visually distinct in the history list and don't skew continuous-mode stats.

### Commands

Two Tauri commands in `commands/audio_upload.rs`:

| Command | Purpose |
|---------|---------|
| `check_audio_ffmpeg` | Verify ffmpeg availability before the user picks a file (shows install instructions if missing). |
| `process_audio_upload` | Run the pipeline, emit progress events, return `UploadedSession` with the first encounter's session ID. |

### Fail-open SOAP generation

If SOAP generation fails for any encounter (LLM timeout, malformed response), the encounter is still archived with its transcript — the user can trigger manual SOAP generation later via the existing retry UI. The upload as a whole does not fail because of SOAP failures on individual encounters. This mirrors the continuous-mode orphaned-SOAP recovery strategy.

### ffmpeg PATH handling

`commands::audio_upload` uses `std::process::Command::new("ffmpeg")` which resolves via the app's inherited `PATH`. On macOS, Tauri apps launched from Finder inherit a *minimal* PATH that may exclude `/opt/homebrew/bin` and `/usr/local/bin` — where ffmpeg is typically installed. `check_audio_ffmpeg` runs `which ffmpeg` with an augmented PATH (`/opt/homebrew/bin:/usr/local/bin:<existing>`) and, if found, the found absolute path is used for subsequent invocations. If ffmpeg is not found on any of these paths, the UI shows a helpful install message rather than a cryptic "command not found" error.

## Consequences

### Positive

- **No algorithmic divergence** between desktop upload and mobile processing: both go through `audio_processing.rs`.
- **Uploaded sessions integrate with existing history, SOAP regeneration, billing extraction, and EMR sync** — they're regular archive entries with a different `charting_mode`.
- **Reuses continuous-mode encounter splitting**: a multi-encounter recording (e.g., a day's worth of visits from a backup recorder) gets auto-split into separate sessions.

### Negative

- **No real-time feedback during transcoding or transcription** — progress is event-based but coarse (stage transitions). For long uploads (1-hour recordings), the user waits without a progress-percentage bar.
- **Assumes ffmpeg on PATH** — the app doesn't bundle ffmpeg (licensing + bundle size). Users on fresh machines must install it once via `brew install ffmpeg`.
- **Server load on bulk uploads** — a physician uploading a month of recordings at once will serialize STT and LLM calls. Acceptable because uploads are rare; if this changes, add a queue.

## References

- `tauri-app/src-tauri/src/audio_processing.rs` — shared pipeline helpers
- `tauri-app/src-tauri/src/commands/audio_upload.rs` — desktop Tauri commands
- `tauri-app/src-tauri/src/bin/process_mobile.rs` — CLI version that shares `audio_processing`
- `tauri-app/src/hooks/useAudioUpload.ts` + `src/components/AudioUploadModal.tsx` — UI
- `docs/superpowers/specs/2026-04-13-mobile-app-v1-design.md` — origin of the shared pipeline pattern
