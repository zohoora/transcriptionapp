# Claude Code Context

This file provides context for AI coders working on this project.

## Project Overview

A real-time speech-to-text transcription desktop app built with:
- **Frontend**: React + TypeScript + Vite
- **Backend**: Rust + Tauri v2
- **Transcription**: Whisper (via whisper-rs)
- **Speaker Detection**: ONNX-based speaker embeddings + online clustering

Target use case: Clinical ambient scribe for physicians - runs as a compact sidebar alongside EMR systems.

## Architecture

```
React Frontend (sidebar UI)
       │
       │ IPC (invoke/listen)
       ▼
Rust Backend
├── commands.rs      # Tauri command handlers
├── session.rs       # Recording state machine
├── audio.rs         # Audio capture, resampling
├── vad.rs           # Voice Activity Detection
├── pipeline.rs      # Processing pipeline coordination
├── config.rs        # Settings persistence
├── transcription.rs # Segment/utterance types
├── models.rs        # Model download management
├── diarization/     # Speaker detection
│   ├── mod.rs       # Embedding extraction (ONNX)
│   ├── clustering.rs # Online speaker clustering
│   └── config.rs    # Clustering parameters
├── enhancement/     # Speech enhancement (GTCRN)
│   ├── mod.rs       # Module exports
│   └── provider.rs  # ONNX-based denoising
├── emotion/         # Emotion detection (wav2small)
│   ├── mod.rs       # Module exports
│   └── provider.rs  # ONNX-based ADV detection
├── ollama.rs        # Ollama LLM client for SOAP note generation
└── biomarkers/      # Vocal biomarker analysis
    ├── mod.rs       # Types (CoughEvent, VocalBiomarkers, SessionMetrics, AudioQualitySnapshot)
    ├── config.rs    # BiomarkerConfig
    ├── thread.rs    # Sidecar processing thread
    ├── audio_quality.rs  # Real-time audio quality metrics
    ├── yamnet/      # YAMNet cough detection (ONNX)
    ├── voice_metrics/
    │   ├── vitality.rs   # F0 pitch variability (mcleod)
    │   └── stability.rs  # CPP via cepstral analysis (rustfft)
    └── session_metrics/  # Turn-taking, talk time ratios
```

## Key IPC Commands

| Command | Purpose |
|---------|---------|
| `run_checklist` | Run pre-flight checks before recording |
| `start_session` | Begin recording with device ID |
| `stop_session` | Stop recording, finalize transcript |
| `reset_session` | Clear transcript, return to idle |
| `list_input_devices` | Get available microphones |
| `check_model_status` | Verify Whisper model availability |
| `get_model_info` | Get info about all models |
| `get_settings` | Retrieve current settings |
| `set_settings` | Update settings |
| `download_whisper_model` | Download Whisper model |
| `download_speaker_model` | Download speaker diarization model |
| `download_enhancement_model` | Download GTCRN enhancement model |
| `download_emotion_model` | Download wav2small emotion model |
| `download_yamnet_model` | Download YAMNet cough detection model |
| `ensure_models` | Download all required models |
| `check_ollama_status` | Check Ollama server connection and list models |
| `list_ollama_models` | Get available models from Ollama |
| `generate_soap_note` | Generate SOAP note from transcript via Ollama |

## Key Events (Backend → Frontend)

| Event | Payload |
|-------|---------|
| `session_status` | `{ state, provider, elapsed_ms, is_processing_behind, error_message? }` |
| `transcript_update` | `{ finalized_text, draft_text, segment_count }` |
| `biomarker_update` | `{ cough_count, cough_rate_per_min, turn_count, vitality_session_mean, stability_session_mean, ... }` |
| `audio_quality` | `{ timestamp_ms, peak_db, rms_db, snr_db, clipped_ratio, dropout_count, ... }` |

## Session States

```
Idle → Preparing → Recording → Stopping → Completed
  ↑                    ↓           ↓          ↓
  └────── Error ←──────┴───────────┴──────────┘
  ↑                                            │
  └─────────────── Reset ←─────────────────────┘
```

## Running the App

**IMPORTANT**: Use debug build, NOT `tauri dev`, for proper deep link and single-instance handling.

```bash
# Build debug app (RECOMMENDED)
pnpm tauri build --debug

# Run with ONNX Runtime (required for transcription, diarization, enhancement)
ORT_DYLIB_PATH=$(./scripts/setup-ort.sh) \
  "src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"

# Or find ORT path manually:
ORT_PATH=$(find ~/.transcriptionapp/ort-venv -name "libonnxruntime.*.dylib" | head -1)
ORT_DYLIB_PATH="$ORT_PATH" \
  "src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"
```

**Note**: Do NOT use `open` to launch the app - it won't inherit environment variables. Run the binary directly with `ORT_DYLIB_PATH` set.

**Why not `tauri dev`?**
- `tauri dev` runs the Vite dev server separately, which breaks deep link routing
- The `tauri-plugin-single-instance` doesn't work correctly in dev mode
- OAuth callbacks (e.g., `fabricscribe://oauth/callback`) open new app instances instead of routing to the existing one
- The debug build bundles everything properly and registers URL schemes correctly

**Deep Link / OAuth Flow**
- App registers `fabricscribe://` URL scheme via `tauri-plugin-deep-link`
- `tauri-plugin-single-instance` ensures only one app instance runs
- When OAuth redirects to `fabricscribe://oauth/callback`, the callback routes to the existing instance
- The frontend listens for `deep-link` events to handle the OAuth code exchange

## Test Commands

```bash
# Frontend tests (Vitest)
pnpm test:run          # Run once
pnpm test              # Watch mode
pnpm test:coverage     # With coverage

# Rust tests
cd src-tauri
cargo test             # Unit tests (needs ORT_DYLIB_PATH for diarization tests)
cargo test --release stress_test  # Stress tests
cargo bench            # Benchmarks
```

Note: Some diarization tests require ONNX Runtime. Set `ORT_DYLIB_PATH` environment variable if tests fail with "onnxruntime library not found".

## SOAP Note Generation (Dec 2024)

Integration with Ollama LLM for generating structured SOAP (Subjective, Objective, Assessment, Plan) notes from clinical transcripts.

**Features**
- Configurable Ollama server URL and model selection
- Default model: qwen3:4b
- "Generate SOAP Note" button appears when session is completed
- Collapsible SOAP section with copy functionality
- Connection status indicator in settings

**Architecture**
- `ollama.rs`: Async HTTP client using reqwest for Ollama API
- API endpoints used:
  - `GET /api/tags` - List available models
  - `POST /api/generate` - Generate SOAP note (stream: false)
- Structured prompt with explicit section markers for reliable parsing
- Handles Qwen's `/think` block output

**Configuration**
- `ollama_server_url`: Ollama server address (default: `http://localhost:11434`)
- `ollama_model`: Model to use (default: `qwen3:4b`)
- Settings persisted in `~/.transcriptionapp/config.json`

**UI Flow**
1. Complete a recording session with transcript
2. SOAP Note section appears below transcript
3. Click "Generate SOAP Note" (requires Ollama connection)
4. Loading spinner during generation (~10-30s depending on model)
5. Structured display of S/O/A/P sections
6. Copy button to copy entire note

## Recent Changes (Dec 2024)

### UI Redesign
- Converted from centered card layout to compact 300px sidebar
- Light mode color scheme for clinical settings
- Collapsible transcript section
- Settings moved to slide-out drawer
- Added speaker detection toggle and max speakers slider

### Speaker Diarization
- Added `diarization/` module for speaker detection
- Uses ONNX model for speaker embeddings
- Online clustering with EMA centroid updates
- Configurable similarity threshold (0.5 default)
- Max speakers limit (2-10, user configurable)

### Speech Enhancement (GTCRN)
- Ultra-lightweight denoising (~523KB model)
- ~2ms latency, 48K parameters
- Enabled by default for cleaner transcriptions
- Model auto-downloads from sherpa-onnx releases

### Emotion Detection (wav2small)
- Dimensional emotion: Arousal, Dominance, Valence (ADV)
- ~120KB model, ~9ms latency, 72K parameters
- Labels: excited/happy, angry/frustrated, calm/content, sad/tired
- Emotion stored in transcript segments

### Biomarker Analysis
Real-time vocal biomarker extraction running in parallel with transcription:

**Vitality (Prosody/Emotional Engagement)**
- Measures pitch variability (F0 standard deviation)
- Detects "flat affect" (Depression/PTSD indicator)
- Uses `pitch-detection` crate (mcleod algorithm) - pure Rust, no ONNX

**Stability (Neurological Control)**
- Measures vocal fold regularity via CPP (Cepstral Peak Prominence)
- Detects fatigue or tremors (Parkinson's indicator)
- Uses `rustfft` for cepstral analysis - pure Rust, no ONNX
- Note: CPP is more robust than jitter/shimmer in ambient noise

**Cough Detection (YAMNet)**
- 521-class audio event classification
- Sliding window: 1s with 500ms hop
- Detects coughs, sneezes, throat clearing
- ~3MB ONNX model (optional - vitality/stability work without it)

**Session Metrics**
- Speaker talk time ratios
- Turn count and average duration
- Cough rate per minute

**Conversation Dynamics**
Real-time analysis of conversation flow between speakers:
- **Overlap detection**: When speaker B starts before speaker A ends
- **Interruption count**: Significant overlap (>500ms)
- **Response latency**: Mean time from speaker A ending to speaker B starting
- **Silence statistics**: Long pause count (>2s), total silence time, silence ratio
- **Engagement score**: 0-100 heuristic combining balance, response speed, turn frequency
- Per-speaker turn statistics (count, mean/median duration)

**Architecture**
- Sidecar thread runs in parallel with transcription
- Clone Point 1: After resample → YAMNet (all audio)
- Clone Point 2: At utterance → Vitality/Stability (original unenhanced audio)
- Non-blocking: biomarker processing doesn't affect transcription latency
- Real-time UI display via `biomarker_update` event (throttled to 2Hz)

**UI Display**
- Collapsible biomarkers section between Record and Transcript sections
- Vitality/Stability shown as color-coded progress bars (green/yellow/red)
- Cough count badge with rate per minute
- Session metrics (turns, balance) when diarization enabled
- Collapsible conversation dynamics section (shown when 2+ speakers detected)
  - Response latency with color-coded status (green <500ms, yellow <1500ms, red ≥1500ms)
  - Overlap/interruption counts
  - Long pause count
  - Engagement score progress bar (0-100)
- Toggle in settings drawer to show/hide biomarkers panel

### Audio Quality Metrics
Real-time audio quality analysis to predict transcript reliability:

**Tier 1 Metrics (Ultra-cheap, O(1) per sample)**
- Peak level (dBFS) - detect clipping risk
- RMS level (dBFS) - detect too quiet/loud
- Clipping count - samples at ±0.98
- Dropout counter - buffer overruns

**Tier 2 Metrics (Cheap, O(N) per chunk)**
- Noise floor estimate - ambient noise level
- SNR estimate - signal-to-noise ratio (uses VAD)
- Silence ratio - fraction of silence frames

**Thresholds**
| Metric | Good | Warning | Poor |
|--------|------|---------|------|
| RMS Level | -40 to -6 dB | < -40 or > -6 dB | - |
| SNR | > 15 dB | 10-15 dB | < 10 dB |
| Clipping | 0% | 0-0.1% | > 0.1% |

**UI Display**
- Collapsible "Audio Quality" section between Record and Biomarkers
- Level and SNR shown as color-coded progress bars
- Status badge: Good (green) / Fair (yellow) / Poor (red)
- Contextual suggestions: "Move microphone closer", "Reduce background noise", etc.
- Clips/Drops counts shown only when > 0

**Architecture**
- `AudioQualityAnalyzer` in `biomarkers/audio_quality.rs`
- Integrated into biomarker thread (parallel processing)
- VAD state passed from pipeline for SNR calculation
- Snapshots emitted every 500ms via `audio_quality` event

### Launch Sequence Checklist
- Pre-flight verification system in `checklist.rs`
- Checks: audio devices, models, configuration
- Status types: Pass, Fail, Warning, Skipped
- Extensible for future features (see module docs)

### Test Updates
- All frontend tests updated for new sidebar UI (131 tests)
- Audio quality tests: 16 Rust unit tests, 12 frontend tests
- Fixed clustering.rs bug where max_speakers wasn't enforced
- All Rust tests passing (243 tests)
- Added mocks for AuthProvider/useAuth hook in test setup

### Conversation Dynamics (Dec 2024)
- Real-time analysis of conversation flow between speakers
- Overlap/interruption detection from segment timing
- Response latency tracking (mean time between speaker transitions)
- Silence statistics (long pauses >2s, silence ratio)
- Engagement score (0-100 heuristic)
- Collapsible UI section between Biomarkers and Transcript
- 17 new Rust unit tests for SessionAggregator

## Settings Schema

```typescript
interface Settings {
  whisper_model: string;      // 'tiny' | 'base' | 'small' | 'medium' | 'large'
  language: string;           // 'en', 'fa', 'auto', etc.
  input_device_id: string | null;
  output_format: string;
  vad_threshold: number;
  silence_to_flush_ms: number;
  max_utterance_ms: number;
  diarization_enabled: boolean;
  max_speakers: number;       // 2-10
  ollama_server_url: string;  // e.g., 'http://localhost:11434'
  ollama_model: string;       // e.g., 'qwen3:4b'
  medplum_server_url: string; // e.g., 'http://localhost:8103'
  medplum_client_id: string;  // OAuth client ID from Medplum
  medplum_auto_sync: boolean; // Auto-sync encounters after recording
}
```

## File Locations

- **All models**: `~/.transcriptionapp/models/`
  - Whisper: `ggml-{tiny,base,small,medium,large}.bin`
  - Speaker: `speaker_embedding.onnx` (~26MB)
  - Enhancement: `gtcrn_simple.onnx` (~523KB)
  - Emotion: `wav2small.onnx` (~120KB)
  - YAMNet: `yamnet.onnx` (~3MB) - for cough detection
- **Settings**: `~/.transcriptionapp/config.json`
- **Medplum Auth**: `~/.transcriptionapp/medplum_auth.json` - persisted OAuth tokens
- **Logs**: Console (tracing crate)

## Common Issues

1. **"Model not found"**: Ensure Whisper model file exists at configured path
2. **ONNX tests failing**: Set `ORT_DYLIB_PATH` to ONNX Runtime library
3. **Audio device errors**: Check microphone permissions (macOS: System Settings → Privacy)
4. **OAuth opens new app instance**: Use `pnpm tauri build --debug` instead of `tauri dev`. The single-instance plugin doesn't work in dev mode.
5. **Medplum auth fails**: Verify `medplum_client_id` in `config.rs` matches your Medplum ClientApplication. Delete `~/.transcriptionapp/config.json` to reset to defaults.
6. **Deep links not working**: Ensure app was built (not running via `tauri dev`). Check that `fabricscribe://` URL scheme is registered in Info.plist.

## Adding New Features

When adding a new feature that requires models or external resources:

1. **Add to Config** (`config.rs`):
   - Add `feature_enabled: bool` field
   - Add `feature_model_path: Option<PathBuf>` if needed
   - Add `get_feature_model_path()` helper

2. **Add Model Download** (`models.rs`):
   - Add `FEATURE_MODEL_URL` constant
   - Add `ensure_feature_model()` function
   - Add `is_feature_model_available()` function
   - Update `get_model_info()` to include the model

3. **Add to Checklist** (`checklist.rs`):
   - Add check in `run_model_checks()` or create new category
   - Return appropriate `CheckStatus` based on config

4. **Add Tauri Command** (`commands.rs`):
   - Add `download_feature_model()` command
   - Register in `lib.rs` invoke_handler

5. **Add to Pipeline** (`pipeline.rs`):
   - Add feature-gated provider initialization
   - Integrate into processing loop
   - Add to drop order at end

## Medplum EMR Integration (Dec 2024)

Integration with Medplum FHIR server for storing encounters, transcripts, SOAP notes, and audio recordings.

**Authentication**
- OAuth 2.0 + PKCE flow via `fabricscribe://oauth/callback` deep link
- Uses `prompt=none` to skip consent screen on subsequent logins (after first consent)
- Session persistence: tokens saved to `~/.transcriptionapp/medplum_auth.json`
- Auto-restore on app startup with automatic token refresh if expired
- Auto-refresh before expiration during session
- Configuration in `config.rs`: `medplum_server_url`, `medplum_client_id`

**FHIR Resources Used**
- `Encounter` - Recording session with start/end times, tagged with `urn:fabricscribe|scribe-session`
- `DocumentReference` - Transcript and SOAP note documents
- `Media` - Audio recording (WAV file stored as Binary)

**Key Commands**
| Command | Purpose |
|---------|---------|
| `medplum_try_restore_session` | Restore saved session, auto-refresh if expired |
| `medplum_start_auth` | Initiate OAuth flow, returns auth URL |
| `medplum_handle_callback` | Exchange code for tokens |
| `medplum_get_auth_state` | Check if authenticated |
| `medplum_logout` | Clear tokens and delete saved session |
| `medplum_sync_encounter` | Upload transcript/SOAP/audio to Medplum |
| `medplum_get_encounter_history` | List past encounters by date range |
| `medplum_get_encounter_details` | Get full encounter with transcript/SOAP/audio |
| `medplum_get_audio_data` | Fetch audio Binary for playback |

**Session History Window**
- Separate Tauri window opened via calendar icon in header
- Calendar component for date selection
- Lists encounters for selected date
- Detail view shows transcript, SOAP note, audio player
- Files: `history.html`, `src/history.tsx`, `src/components/HistoryWindow.tsx`, `src/components/Calendar.tsx`, `src/components/AudioPlayer.tsx`

**Vite Multi-Page Build**
- `vite.config.ts` configured with rollup input for both `index.html` and `history.html`
- History window created via `WebviewWindow` API from `@tauri-apps/api/webviewWindow`

## ADRs

See `docs/adr/` for Architecture Decision Records:
- 0001: Use Tauri for desktop app
- 0002: Whisper for transcription
- 0003: VAD-gated processing
- 0004: Ring buffer audio pipeline
- 0005: Session state machine
- 0006: Speaker diarization (online clustering)
