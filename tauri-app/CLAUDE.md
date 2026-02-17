# Claude Code Context

Clinical ambient scribe for physicians - real-time speech-to-text transcription desktop app.

## Tech Stack

- **Frontend**: React + TypeScript + Vite
- **Backend**: Rust + Tauri v2
- **Transcription**: STT Router (WebSocket streaming to Whisper backend, alias-based routing)
- **Speaker Detection**: ONNX-based speaker embeddings + online clustering
- **LLM**: OpenAI-compatible API for SOAP note generation
- **EMR**: Medplum FHIR server integration

## Architecture

```
React Frontend (sidebar UI)
       │
       │ IPC (invoke/listen)
       ▼
Rust Backend
├── commands/          # Tauri command handlers
│   ├── mod.rs             # Re-exports, CommandError, PipelineState
│   ├── session.rs         # Recording lifecycle + auto-end archive
│   ├── settings.rs        # get/set settings
│   ├── audio.rs           # Device enumeration
│   ├── models.rs          # Model download commands
│   ├── ollama.rs          # LLM router connection
│   ├── medplum.rs         # EMR sync commands
│   ├── listening.rs       # Auto-session detection commands
│   ├── speaker_profiles.rs # Speaker enrollment CRUD
│   ├── clinical_chat.rs   # Clinical assistant chat proxy
│   ├── miis.rs            # Medical illustration proxy
│   ├── screenshot.rs      # Screen capture commands
│   ├── continuous.rs      # Continuous charting mode commands
│   ├── archive.rs         # Local session history commands
│   ├── whisper_server.rs  # STT Router status commands
│   └── permissions.rs     # Microphone permission commands
├── session.rs         # Recording state machine
├── audio.rs           # Audio capture, resampling
├── vad.rs             # Voice Activity Detection
├── pipeline.rs        # Processing pipeline + silence tracking
├── config.rs          # Settings persistence
├── llm_client.rs      # OpenAI-compatible LLM client
├── medplum.rs         # FHIR client (OAuth, encounters)
├── whisper_server.rs  # STT Router client (WebSocket streaming + batch fallback)
├── transcription.rs   # Segment and utterance types
├── models.rs          # Model download management
├── checklist.rs       # Pre-flight verification system
├── listening.rs       # Auto-session detection
├── speaker_profiles.rs # Speaker enrollment storage
├── local_archive.rs   # Local session storage
├── continuous_mode.rs # Continuous charting mode (end-of-day)
├── screenshot.rs      # Screen capture (in-memory JPEG)
├── debug_storage.rs   # Debug storage (dev only)
├── permissions.rs     # macOS permission checks
├── ollama.rs          # Re-exports from llm_client.rs (backward compat)
├── activity_log.rs    # Structured PHI-safe activity logging
├── encounter_experiment.rs # Encounter detection experiment CLI support
├── vision_experiment.rs    # Vision SOAP experiment CLI support
├── diarization/       # Speaker detection (ONNX embeddings, clustering)
├── enhancement/       # Speech enhancement (GTCRN)
├── biomarkers/        # Vocal analysis (vitality, stability, cough detection)
├── mcp/               # MCP server on port 7101
└── preprocessing.rs   # DC removal, high-pass filter, AGC
```

## Quick Start

```bash
# Build (use debug build, NOT tauri dev - required for OAuth deep links)
pnpm tauri build --debug

# Bundle ONNX Runtime (one-time after build)
./scripts/bundle-ort.sh "src-tauri/target/debug/bundle/macos/Transcription App.app"

# Run
open "src-tauri/target/debug/bundle/macos/Transcription App.app"

# Verify
npx tsc --noEmit                 # TypeScript typecheck
cd src-tauri && cargo check      # Rust compile check

# Tests
pnpm test:run                    # Frontend (Vitest)
cd src-tauri && cargo test       # Rust

# Daily preflight (verifies STT + LLM + Archive before clinic)
./scripts/preflight.sh           # Quick (~10s): layers 1-3
./scripts/preflight.sh --full    # Full (~30s): all 5 layers
```

**Why not `tauri dev`?** Deep links and single-instance plugin don't work in dev mode. OAuth callbacks open new instances instead of routing to existing app.

## Key Files for Common Tasks

| Task | Files to Modify |
|------|-----------------|
| Add new setting | `config.rs`, `types/index.ts`, `useSettings.ts`, `SettingsDrawer.tsx` |
| Modify transcription | `pipeline.rs`, `whisper_server.rs` (STT Router streaming), `transcription.rs` (types) |
| Change SOAP prompt | `llm_client.rs` (`build_multi_patient_soap_prompt()`) |
| Modify LLM integration | `llm_client.rs`, `commands/ollama.rs`, `useOllamaConnection.ts` |
| Add new biomarker | `biomarkers/mod.rs`, `BiomarkersSection.tsx` |
| Modify biomarker thresholds | `types/index.ts` (`BIOMARKER_THRESHOLDS`, status helper functions) |
| Modify UI modes | `components/modes/` (ReadyMode, RecordingMode, ReviewMode) |
| Add Tauri command | `commands/*.rs`, register in `lib.rs` |
| Modify Medplum sync | `commands/medplum.rs`, `useMedplumSync.ts`, `App.tsx` |
| Modify auto-detection | `listening.rs`, `commands/listening.rs`, `useAutoDetection.ts` |
| Modify speaker enrollment | `speaker_profiles.rs`, `commands/speaker_profiles.rs`, `useSpeakerProfiles.ts`, `SpeakerEnrollment.tsx` |
| Modify clinical chat | `commands/clinical_chat.rs`, `useClinicalChat.ts`, `ClinicalChat.tsx` |
| Modify auto-end detection | `pipeline.rs` (silence tracking), `config.rs` (settings), `useSessionState.ts` |
| Modify SOAP options | `useSoapNote.ts` (hook), `llm_client.rs` (prompt building), `local_archive.rs` (metadata) |
| Modify MIIS integration | `commands/miis.rs`, `useMiisImages.ts`, `ImageSuggestions.tsx`, `usePredictiveHint.ts` |
| Modify continuous mode | `continuous_mode.rs`, `commands/continuous.rs`, `useContinuousMode.ts`, `ContinuousMode.tsx` |
| Modify patient biomarkers | `usePatientBiomarkers.ts`, `PatientPulse.tsx`, `PatientVoiceMonitor.tsx` |
| Add session-scoped state | `useSessionLifecycle.ts` (add reset call to `resetAllSessionState`) |

## IPC Commands

| Command | Purpose |
|---------|---------|
| `start_session` / `stop_session` / `reset_session` | Recording lifecycle |
| `run_checklist` | Pre-flight checks |
| `list_input_devices` | Available microphones |
| `get_settings` / `set_settings` | Configuration |
| `get_audio_file_path` | Get audio file path for playback |
| `reset_silence_timer` | Cancel auto-end countdown |
| `check_model_status` / `get_model_info` / `ensure_models` | Model availability |
| `download_whisper_model` / `download_speaker_model` / `download_enhancement_model` / `download_yamnet_model` | Model downloads |
| `get_whisper_models` / `download_whisper_model_by_id` / `test_whisper_model` / `is_model_downloaded` | Whisper model management |
| `check_ollama_status` / `list_ollama_models` / `prewarm_ollama_model` | LLM router connection |
| `generate_soap_note` / `generate_soap_note_auto_detect` | SOAP generation (single + multi-patient) |
| `generate_predictive_hint` | LLM hints during recording |
| `medplum_get_auth_state` / `medplum_start_auth` / `medplum_handle_callback` / `medplum_logout` | Medplum auth lifecycle |
| `medplum_try_restore_session` / `medplum_refresh_token` / `medplum_check_connection` | Medplum session management |
| `medplum_search_patients` / `medplum_create_encounter` / `medplum_complete_encounter` | Patient/encounter CRUD |
| `medplum_get_encounter_history` / `medplum_get_encounter_details` / `medplum_get_audio_data` | Encounter history |
| `medplum_sync_encounter` / `medplum_quick_sync` / `medplum_add_soap_to_encounter` | EMR sync |
| `medplum_multi_patient_quick_sync` | Multi-patient encounter sync |
| `check_whisper_server_status` / `list_whisper_server_models` | STT Router status |
| `check_microphone_permission` / `request_microphone_permission` / `open_microphone_settings` | Microphone permissions |
| `start_listening` / `stop_listening` / `get_listening_status` | Auto-session detection |
| `list_speaker_profiles` / `get_speaker_profile` / `create_speaker_profile` | Speaker enrollment management |
| `update_speaker_profile` / `delete_speaker_profile` / `reenroll_speaker_profile` | Speaker profile CRUD |
| `get_local_session_dates` / `get_local_sessions_by_date` / `get_local_session_details` | Local archive history |
| `save_local_soap_note` / `read_local_audio_file` | Local archive operations (path-validated) |
| `clinical_chat_send` | Send message to clinical assistant LLM |
| `miis_suggest` / `miis_send_usage` | Medical illustration suggestions + telemetry |
| `check_screen_recording_permission` / `open_screen_recording_settings` | Screen recording permissions |
| `start_screen_capture` / `stop_screen_capture` / `get_screen_capture_status` | Screen capture lifecycle |
| `get_screenshot_paths` / `get_screenshot_thumbnails` | Screenshot retrieval |
| `generate_vision_soap_note` | Vision-based SOAP (experimental) |
| `run_vision_experiments` / `get_vision_experiment_results` / `get_vision_experiment_report` / `list_vision_experiment_strategies` | Vision experiments |
| `start_continuous_mode` / `stop_continuous_mode` | Continuous charting mode lifecycle |
| `get_continuous_mode_status` | Get continuous mode stats (state, encounters, buffer) |
| `trigger_new_patient` | Force new encounter boundary in continuous mode |
| `set_continuous_encounter_notes` | Set clinician notes for current encounter |

## Events (Backend → Frontend)

| Event | Purpose |
|-------|---------|
| `session_status` | Recording state changes |
| `transcript_update` | Real-time transcript (session mode only) |
| `biomarker_update` | Vitality, stability, session metrics |
| `audio_quality` | Level, SNR, clipping metrics |
| `listening_event` | Auto-detection status (includes `speaker_not_verified`) |
| `silence_warning` | Auto-end countdown (silence_ms, remaining_ms) |
| `session_auto_end` | Session auto-ended due to silence |
| `continuous_mode_event` | Continuous mode status changes (started, encounter_detected, soap_generated, encounter_merged, etc.) |
| `continuous_transcript_preview` | Live transcript preview in continuous mode (separate from `transcript_update`) |

## Session States

```
Idle → Preparing → Recording → Stopping → Completed
  ↑                                           │
  └──────────── Reset / Error ←───────────────┘
```

## Code Patterns & Gotchas

| Pattern | Rule |
|---------|------|
| Concurrent async guards | Use `useRef` (not `useState`) — state is async, can't prevent double-clicks |
| Session lifecycle resets | Add cleanup to `useSessionLifecycle.resetAllSessionState()` |
| Pipeline staleness | Generation counter (`u64`) — discard messages from previous pipeline runs |
| Tauri `listen()` cleanup | Use `mounted` flag + call `fn()` immediately if unmounted before resolve |
| Pipeline handle cleanup | `PipelineHandle` has `Drop` impl; spawn background thread for `h.join()` to avoid blocking Tauri thread |
| Config safety | Clamp loaded values to safe ranges (user could edit JSON manually) |
| Date arithmetic | Use `checked_add_signed` not `+` for chrono dates (prevents panic) |
| UTF-8 slicing | Use `ceil_char_boundary()` for safe substring truncation |
| Serde casing | All frontend-facing structs need `#[serde(rename_all = "camelCase")]` |
| Event namespacing | Session events (`transcript_update`) vs continuous events (`continuous_transcript_preview`) — never share |
| File read commands | Always validate paths are within expected directories (path traversal prevention) |
| Emit after success | Don't emit "started" events before the operation actually succeeds |
| Functional setState | Use `prev => !prev` pattern in `useCallback` to avoid stale closures |
| Vec batch cleanup | Use `drain(..excess)` not `remove(0)` in loop |
| Path traversal prevention | `validate_session_id()` in local_archive.rs, `validate_fhir_id()` in medplum.rs — reject `..` and non-alphanumeric IDs |
| Error body truncation | `truncate_error_body()` in medplum.rs — prevents huge HTML error pages from flooding logs |
| Token refresh locking | `get_valid_token()` in medplum.rs — double-check locking pattern to avoid concurrent refresh races |
| Settings validation after update | `clamp_values()` called after `update_from_settings()` in config.rs — safety net for user-edited JSON |
| Encounter notes: clone before clear | In continuous mode detector, clone accumulated notes before clearing buffer to avoid data loss |
| Audio quality shared util | `getAudioQualityLevel()` in utils.ts — shared across RecordingMode, ReviewMode, ContinuousMode |

## Features

### SOAP Note Generation
- OpenAI-compatible API (`/v1/chat/completions`)
- Multi-patient support: LLM auto-detects patients vs physician
- Audio events (coughs, laughs) passed to LLM for clinical context
- Adaptive model selection: `soap_model_fast` for <5K words, `soap_model` for longer
- Long transcript support: 20/80 truncation strategy for sessions >10K words
- **Auto-copy**: SOAP notes automatically copied to clipboard on generation
- **Format options**: Problem-based (organizes by medical problem) vs Comprehensive (unified sections)
- **Detail level**: 1-10 slider controls verbosity (persisted across sessions)
- **Session metadata**: SOAP options stored with local archive sessions for regeneration context

### Transcription
- **STT Router streaming**: WebSocket connection to STT Router using named aliases (e.g., `medical-streaming`)
- Alias-based routing: `stt_alias` selects the backend model/pipeline, `stt_postprocess` enables medical term correction
- All modes (session, continuous, listening) use streaming — no batch transcription
- Audio preprocessing: DC removal, 80Hz high-pass, AGC

### Auto-Session Detection
- VAD monitors for sustained speech (2+ seconds)
- Optimistic recording starts immediately, greeting check runs in parallel
- If greeting confirmed → recording continues; if rejected → discarded
- **Speaker Verification** (optional): Only auto-start for enrolled speakers
  - `auto_start_require_enrolled`: Require voice to match a speaker profile
  - `auto_start_required_role`: Optionally require specific role (e.g., Physician only)

### Medplum EMR Integration
- OAuth 2.0 + PKCE via `fabricscribe://oauth/callback`
- Auto-sync: transcript + audio synced on session complete
- SOAP auto-added to existing encounter (patient ID resolved from `Encounter.subject.reference`)

### Biomarkers
Vitality (F0 pitch variability), Stability (CPP), Cough Detection (YAMNet ONNX), Conversation Dynamics (turn-taking, engagement score). Thresholds and status helpers (`getVitalityStatus()`, etc.) in `types/index.ts`.

### Speaker Enrollment
Voice profiles (name, role, 256-dim ECAPA-TDNN embedding) stored in `speaker_profiles.json`. Enrolled speakers matched first (threshold 0.6) → fall back to auto-clustering (0.3). Speaker context injected into SOAP prompts. See ADR 0014.

### Clinical Assistant Chat
Real-time LLM chat during recording via `clinical-assistant` model alias. Backend proxies HTTP through Rust (bypasses CSP). Router must handle tool execution server-side — without it, raw tool call JSON is shown instead of results. See ADR 0017.

### Auto-End Silence Detection
Automatically ends recording sessions after prolonged silence.

**Configuration**: `auto_end_enabled` setting + `auto_end_silence_ms` (default: 180000ms = 3 minutes)

**Flow**:
1. VAD tracks continuous silence during recording
2. After threshold reached, emits `SilenceWarning` with countdown
3. User can cancel via "Keep Recording" button (calls `reset_silence_timer`)
4. If not cancelled, session auto-stops gracefully
5. Auto-ended sessions are archived with `auto_ended: true` and `auto_end_reason: "silence"` metadata
6. Debug storage is also saved for auto-ended sessions (if enabled)

**Files**: `pipeline.rs` (silence tracking), `commands/session.rs` (reset command + auto-end archive in `Stopped` handler), `useSessionState.ts` (UI)

### MCP Server
Port 7101, JSON-RPC 2.0. Tools: `agent_identity`, `health_check`, `get_status`, `get_logs`

### MIIS (Medical Illustration Image Server)
LLM extracts medical concepts from transcript every 30s → MIIS server returns ranked image suggestions. Backend proxies through Rust (avoids CORS). Config: `miis_enabled`, `miis_server_url`. Server needs embedder enabled for semantic matching. See ADR 0018.

### Continuous Charting Mode (End of Day)
Records all day without manual start/stop. LLM encounter detector segments transcript into patient encounters and generates SOAP notes automatically. Two modes: **Session** (default, manual) and **Continuous** (end-of-day). See ADR 0019.

**Detection**: Pipeline → TranscriptBuffer → LLM check every `encounter_check_interval_secs` (120s) or on `encounter_silence_trigger_secs` (45s) of silence → archive + SOAP on complete encounter.

**Vision-Based Patient Name Extraction**: When `screen_capture_enabled` is true, a background task captures screenshots every `screen_capture_interval_secs` (min 30s), sends them to the `vision-model` LLM alias, and extracts patient names from on-screen EHR charts. A `PatientNameTracker` uses majority-vote across screenshots to determine the most likely patient for each encounter. The extracted name is written to `ArchiveMetadata.patient_name` and displayed in the dashboard and history views.

**Lifecycle**:
- `started` event emitted only after pipeline successfully starts
- `isActive=false` on `error` events (prevents stale UI state)
- Auto-detection (listening mode) disabled while continuous mode is active
- Charting mode switch to "session" blocked while continuous recording is active
- Transcript preview uses `continuous_transcript_preview` event (separate from session's `transcript_update`)

## Settings Schema

Source of truth: `src-tauri/src/config.rs` (Rust) / `src/types/index.ts` (TypeScript).

Key settings groups: STT Router (whisper_server_url, stt_alias=`"medical-streaming"`, stt_postprocess=true), Audio (VAD, diarization, enhancement), LLM Router (soap_model=`"soap-model-fast"`, soap_model_fast=`"soap-model-fast"`, fast_model=`"fast-model"`), Medplum (OAuth, auto_sync), Auto-detection (auto_start, auto_end_silence_ms=180000), SOAP (detail_level 1-10, format, custom_instructions), MIIS, Screen Capture, Continuous Mode (charting_mode, encounter_check_interval_secs=120, encounter_silence_trigger_secs=45, encounter_merge_enabled, encounter_detection_model=`"faster"`, encounter_detection_nothink=true), Debug.

## File Locations

| Path | Contents |
|------|----------|
| `~/.transcriptionapp/models/` | Whisper, speaker embedding, enhancement, YAMNet models |
| `~/.transcriptionapp/config.json` | Settings |
| `~/.transcriptionapp/speaker_profiles.json` | Enrolled speaker voice profiles |
| `~/.transcriptionapp/medplum_auth.json` | OAuth tokens |
| `~/.transcriptionapp/archive/` | Local session archive (`YYYY/MM/DD/session_id/`) |
| `~/.transcriptionapp/logs/` | Activity logs (daily rotation, PHI-safe) |
| `~/.transcriptionapp/debug/` | Debug storage (dev only) |

## External Services

| Service | Default URL | Purpose |
|---------|-------------|---------|
| STT Router | `http://10.241.15.154:8001` | WebSocket streaming transcription (alias: `medical-streaming`) |
| LLM Router | `http://10.241.15.154:8080` | SOAP generation, encounter detection, vision-based patient name extraction (`vision-model` alias) |
| Medplum | `http://10.241.15.154:8103` | EMR/FHIR |
| MIIS | `http://10.241.15.154:7843` | Medical illustration images |

## Frontend Structure

**Mode Components** (`src/components/modes/`):
- `ReadyMode.tsx` - Pre-recording (checklist, device selection)
- `RecordingMode.tsx` - Active recording (timer, quality, transcript preview)
- `ReviewMode.tsx` - Post-recording (transcript, SOAP, EMR sync)
- `ContinuousMode.tsx` - Continuous charting dashboard (monitoring, live transcript, encounter stats)

**Key Components** (`src/components/`):
- `PatientPulse.tsx` - Glanceable biomarker summary (vitality, stability, cough, engagement)
- `PatientVoiceMonitor.tsx` - Patient-focused voice metric trending
- `SpeakerEnrollment.tsx` - Speaker voice enrollment UI
- `ClinicalChat.tsx` - Clinical assistant chat panel
- `ImageSuggestions.tsx` - MIIS medical illustration display
- `SyncStatusBar.tsx` - EMR sync status indicator

**Key Hooks** (`src/hooks/`):
- `useSessionLifecycle` - Centralized session start/reset coordination across all hooks
- `useSessionState` - Recording state, transcript, biomarkers
- `useSoapNote` - SOAP generation
- `useMedplumSync` - EMR sync with encounter tracking
- `useSettings` - Configuration management
- `useAutoDetection` - Listening mode
- `useSpeakerProfiles` - Speaker enrollment CRUD operations
- `useClinicalChat` - Clinical assistant chat during recording
- `usePredictiveHint` - LLM hints + concept extraction during recording
- `useMiisImages` - Medical illustration suggestions from MIIS server
- `useContinuousMode` - Continuous charting mode state and controls
- `usePatientBiomarkers` - Patient-focused biomarker trending for continuous mode
- `useScreenCapture` - Periodic screenshot capture during recording
- `useChecklist` - Pre-flight system checks
- `useDevices` - Audio input device enumeration
- `useWhisperModels` - Whisper model download and management

**Types**: `src/types/index.ts` - All TypeScript interfaces mirroring Rust structs, biomarker thresholds and status helpers

## Common Issues

| Problem | Solution |
|---------|----------|
| "Model not found" | Check `~/.transcriptionapp/models/` for ONNX models (diarization, enhancement) |
| ONNX tests failing | Set `ORT_DYLIB_PATH` environment variable |
| Audio device errors | Check macOS microphone permissions |
| OAuth opens new instance | Use `pnpm tauri build --debug`, not `tauri dev` |
| Deep links not working | Ensure app was built and `fabricscribe://` scheme registered |
| Clinical chat shows raw JSON | Router must execute tools for `clinical-assistant` alias |
| Speaker verification fails | Ensure profiles exist and speaker model at `~/.transcriptionapp/models/speaker_embedding.onnx` (or legacy `voxceleb_ECAPA512_LM.onnx`) |
| Auto-end too aggressive | Increase `auto_end_silence_ms` or disable `auto_end_enabled` |
| SOAP not copying to clipboard | Check Tauri clipboard plugin permissions |
| MIIS images not loading | Check CSP allows MIIS server domain in `tauri.conf.json` |
| MIIS same images for all queries | Server needs embedder enabled for semantic matching |
| Continuous mode not detecting encounters | Check LLM router connection, increase `encounter_check_interval_secs` |
| Continuous mode UI not showing | Verify `charting_mode: "continuous"` in config.json, restart app |
| "Auto-charted" badge not appearing | Session was created in session mode, not continuous mode |
| Can't switch charting mode | Stop continuous recording before switching from continuous to session mode |
| Auto-detection runs during continuous | Verify `isContinuousMode` guard in App.tsx listening effect |

## E2E Integration Tests

End-to-end tests verify the full pipeline against live STT and LLM Router services. They live in `src-tauri/src/e2e_tests.rs` and are marked `#[ignore]` so they don't run during normal `cargo test`.

### Daily Preflight Script

```bash
./scripts/preflight.sh           # Quick check (~10s) — layers 1-3
./scripts/preflight.sh --full    # Full pipeline (~30s) — all 5 layers
./scripts/preflight.sh --layer 2 # Specific layer only
```

### Running Tests Directly

```bash
cd src-tauri

# All E2E tests (run one at a time — concurrent WebSocket streams can overload STT Router)
cargo test e2e_layer1 -- --ignored --nocapture  # STT Router
cargo test e2e_layer2 -- --ignored --nocapture  # LLM Router
cargo test e2e_layer3 -- --ignored --nocapture  # Local Archive
cargo test e2e_layer4 -- --ignored --nocapture  # Session mode full pipeline
cargo test e2e_layer5 -- --ignored --nocapture  # Continuous mode full pipeline

# Single test
cargo test e2e_layer2_hybrid -- --ignored --nocapture
```

### Test Layers

| Layer | What it Tests | Services Required |
|-------|--------------|-------------------|
| 1 | STT Router health, alias, WebSocket streaming | STT Router |
| 2 | SOAP generation, encounter detection (faster + /nothink), hybrid model + merge + hallucination filter | LLM Router |
| 3 | Archive save/retrieve, continuous mode metadata | Filesystem only |
| 4 | Session mode: Audio → STT → SOAP → Archive → History | STT + LLM Router |
| 5 | Continuous mode: Audio → STT → Detection → SOAP → Archive → History | STT + LLM Router |

### Hybrid Model Configuration

E2E tests use the production model configuration:
- **Detection**: `faster` (Qwen3-1.7B) + `/nothink` — smaller model resists over-splitting
- **Merge**: `fast-model` (~7B) + patient name (M1 strategy) — better semantic understanding
- **SOAP**: `soap-model-fast` — dedicated SOAP generation model

Config fields in `config.rs`: `encounter_detection_model` (default "faster"), `encounter_detection_nothink` (default true)

### Troubleshooting E2E Failures

| Failure | Likely Cause | Fix |
|---------|-------------|-----|
| Layer 1 health check | STT Router down | Check `http://10.241.15.154:8001/health` |
| Layer 1 streaming "Connection reset" | Too many concurrent WebSocket connections | Run tests one layer at a time |
| Layer 2 SOAP empty | LLM Router down or model not loaded | Check `http://10.241.15.154:8080/health` |
| Layer 2 detection not complete | Model changed or prompt regression | Run encounter experiment CLI to compare |
| Layer 2 merge says different | Patient name not in prompt or model regression | Check `build_encounter_merge_prompt()` |
| Layer 3 archive failure | Disk permissions | Check `~/.transcriptionapp/archive/` writable |
| Layer 4/5 "STT returned 4 chars" | Normal — sine wave test audio produces no speech | Test uses fixture transcript as fallback |

### Experiment CLIs

For deeper investigation of model accuracy:

```bash
cd src-tauri

# Encounter detection experiments (replays archived transcripts)
cargo run --bin encounter_experiment_cli
cargo run --bin encounter_experiment_cli -- --model faster --nothink
cargo run --bin encounter_experiment_cli -- --detect-only p0 p3

# Vision SOAP experiments
cargo run --bin vision_experiment_cli
```

## Testing Best Practices

- Avoid `vi.useFakeTimers()` with React async - conflicts with RTL's `waitFor`
- Use `mockImplementation` with command routing instead of `mockResolvedValueOnce` chains
- Always clean up timers in `beforeEach`/`afterEach`
- Run E2E tests one layer at a time to avoid STT Router WebSocket concurrency limits

## Adding New Features

1. **Config**: Add field to `config.rs`, `types/index.ts`, `useSettings.ts`, `SettingsDrawer.tsx`
2. **Model**: Add URL/download in `commands/models.rs`, check in `checklist.rs`
3. **Command**: Add to `commands/*.rs`, register in `lib.rs`
4. **Pipeline**: Add provider initialization in `pipeline.rs`

## ADRs

See `docs/adr/` for Architecture Decision Records:

| ADR | Title |
|-----|-------|
| 0001 | Use Tauri for desktop app |
| 0002 | Whisper for transcription |
| 0003 | VAD-gated processing |
| 0004 | Ring buffer audio pipeline |
| 0005 | Session state machine |
| 0006 | Speaker diarization |
| 0007 | Biomarker analysis |
| 0008 | Medplum EMR integration |
| 0009 | Ollama SOAP generation |
| 0010 | Audio preprocessing |
| 0011 | Auto-session detection |
| 0012 | Multi-patient SOAP generation |
| 0013 | LLM router migration |
| 0014 | Speaker enrollment system |
| 0015 | Auto-end silence detection |
| 0016 | Speaker-verified auto-start |
| 0017 | Clinical assistant chat |
| 0018 | MIIS medical illustration integration |
| 0019 | Continuous charting mode (end of day) |
| 0020 | STT Router streaming integration |
