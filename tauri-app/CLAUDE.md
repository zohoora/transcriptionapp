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
├── preprocessing.rs # Audio preprocessing (DC removal, high-pass, AGC)
├── ollama.rs        # Ollama LLM client for SOAP note generation
├── medplum.rs       # Medplum FHIR client (OAuth, encounters, documents)
├── whisper_server.rs # Remote Whisper server client (faster-whisper)
├── activity_log.rs  # Structured activity logging (PHI-safe)
├── checklist.rs     # Pre-flight verification checks
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
| `download_whisper_model` | Download Whisper model (legacy) |
| `get_whisper_models` | Get all Whisper models with download status |
| `download_whisper_model_by_id` | Download a specific Whisper model |
| `test_whisper_model` | Validate a downloaded model |
| `is_model_downloaded` | Check if a model is downloaded |
| `download_speaker_model` | Download speaker diarization model |
| `download_enhancement_model` | Download GTCRN enhancement model |
| `download_yamnet_model` | Download YAMNet cough detection model |
| `ensure_models` | Download all required models |
| `check_ollama_status` | Check Ollama server connection and list models |
| `list_ollama_models` | Get available models from Ollama |
| `generate_soap_note` | Generate SOAP note from transcript via Ollama |
| `check_whisper_server_status` | Check remote Whisper server connection |
| `list_whisper_server_models` | Get available models from Whisper server |

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

# Bundle ONNX Runtime into the app (one-time after build)
./scripts/bundle-ort.sh "src-tauri/target/debug/bundle/macos/Transcription App.app"

# Run the app (no ORT_DYLIB_PATH needed!)
"src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"

# Or use `open` for bundled apps:
open "src-tauri/target/debug/bundle/macos/Transcription App.app"
```

**Alternative**: Run with external ONNX Runtime (development only):
```bash
ORT_DYLIB_PATH=$(./scripts/setup-ort.sh) \
  "src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"
```

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

## Building for Distribution

For internal/trusted distribution to other Macs:

```bash
# One-command build with ONNX Runtime bundled:
./scripts/build-distributable.sh

# Or for release build:
./scripts/build-distributable.sh --release
```

This creates a self-contained app bundle at:
- Debug: `src-tauri/target/debug/bundle/macos/Transcription App.app`
- Release: `src-tauri/target/release/bundle/macos/Transcription App.app`

**What's bundled:**
- The compiled Rust/Tauri app
- Frontend assets
- ONNX Runtime library (~26MB) in `Contents/Frameworks/`

**Installing on another Mac:**
1. Copy the `.app` bundle or use the `.dmg` installer
2. First launch: Right-click → "Open" to bypass Gatekeeper (unsigned app)
3. Subsequent launches work normally

**Note**: Models (Whisper, speaker embedding, etc.) are NOT bundled - they download on first use to `~/.transcriptionapp/models/`. This keeps the app size reasonable (~50MB vs ~500MB+).

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

## SOAP Note Generation (Dec 2024, Updated Jan 2025)

Integration with Ollama LLM for generating structured SOAP (Subjective, Objective, Assessment, Plan) notes from clinical transcripts.

**Features**
- Configurable Ollama server URL and model selection
- Default model: qwen3:4b
- "Generate SOAP Note" button appears when session is completed
- Collapsible SOAP section with copy functionality
- Connection status indicator in settings
- **Audio events included** (coughs, laughs, sneezes) with confidence scores

**Architecture**
- `ollama.rs`: Async HTTP client using reqwest for Ollama API
- API endpoints used:
  - `GET /api/tags` - List available models
  - `POST /api/generate` - Generate SOAP note (stream: false)
- **JSON output format** for reliable parsing (not text markers)
- Handles Qwen's `<think>` blocks and markdown code fences
- Type-safe parsing with `serde_json`

**Audio Events in SOAP Generation (Jan 2025)**
- YAMNet-detected audio events are passed to the LLM for clinical context
- Events include: Cough, Laughter, Sneeze, Throat clearing, etc.
- Each event includes timestamp and confidence score (converted to percentage)
- Events are NOT displayed in UI (removed) but used for SOAP note generation
- Helps LLM understand patient's condition (e.g., frequent coughing → respiratory issue)

**Prompt Format**
```
You are a medical scribe assistant. Based on the following clinical transcript, generate a SOAP note.

TRANSCRIPT:
[transcript text]

AUDIO EVENTS DETECTED:
- Cough at 0:30 (confidence: 88%)
- Laughter at 1:05 (confidence: 82%)

Respond with ONLY valid JSON:
{
  "subjective": "...",
  "objective": "...",
  "assessment": "...",
  "plan": "..."
}

Rules:
- Only include information explicitly mentioned or directly inferable
- Use "No information available" if a section has no relevant content
- Consider audio events (coughs, laughs, etc.) when relevant to the clinical picture
- Output ONLY the JSON object, no markdown, no explanation
```

Note: The prompt is model-agnostic (works with any Ollama model). The response parser handles Qwen's `<think>` blocks and markdown code fences gracefully.

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

**Data Flow for Audio Events**
```
YAMNet (biomarkers thread)
    → CoughEvent[] (with label, timestamp, confidence)
    → biomarker_update event to frontend
    → Stored in biomarkers.recent_events
    → Passed to generate_soap_note command
    → Formatted in LLM prompt
    → LLM considers events for clinical context
```

## Recent Changes (Jan 2025)

### Audio Events in SOAP Generation
- Audio events (coughs, laughs, sneezes, throat clearing) now sent to LLM
- `AudioEvent` type added to `ollama.rs` with timestamp, duration, confidence, label
- Confidence converted from logits to percentages using sigmoid mapping
- Events formatted in LLM prompt with timestamps (e.g., "Cough at 0:30 (confidence: 88%)")
- Removed cough/audio event display from UI (BiomarkersSection, RecordingMode, ReviewMode)
- Frontend passes `biomarkers.recent_events` to `generate_soap_note` command

### Multi-Window Support
- History window now independent from main app window
- Closing history window no longer closes the entire app
- Fixed in `lib.rs` by checking `window.label() != "main"` before exit

### Enhanced Whisper Model Selection
- Settings dropdown now shows all 17 available Whisper models grouped by category
- Categories: Standard, Large, Quantized, Distil-Whisper
- Each model shows download status (checkmark for downloaded, cloud icon for not downloaded)
- Download button appears when selecting a non-downloaded model
- Models auto-tested after download to verify integrity (GGML magic bytes check)
- Models include:
  - **Standard**: tiny, tiny.en, base, base.en, small, small.en, medium, medium.en
  - **Large**: large-v2, large-v3, large-v3-turbo
  - **Quantized**: large-v3-q5_0 (faster, lower quality), large-v3-turbo-q5_0
  - **Distil-Whisper**: distil-large-v3, distil-large-v3.en (3.5x faster, English-focused)
- Backend: `get_whisper_models`, `download_whisper_model_by_id`, `test_whisper_model` commands
- Frontend: `useWhisperModels` hook, updated `SettingsDrawer`

### Remote Whisper Server Support
Option to run transcription on a remote server for devices with limited RAM/CPU.

**Architecture**
- `whisper_server.rs`: HTTP client for faster-whisper-server (OpenAI-compatible API)
- `TranscriptionProvider` enum in `pipeline.rs`: Abstracts local vs remote transcription
- WAV encoding: Converts f32 audio samples to WAV bytes for HTTP transmission
- Blocking async wrapper pattern (similar to Ollama client)

**Configuration**
- `whisper_mode`: "local" (default) or "remote"
- `whisper_server_url`: Server address (default: `http://192.168.50.149:8000`)
- `whisper_server_model`: Model to use (default: `large-v3-turbo`)

**API**
Uses OpenAI-compatible `/v1/audio/transcriptions` endpoint:
```bash
POST /v1/audio/transcriptions
Content-Type: multipart/form-data
file=@audio.wav
model=large-v3-turbo
language=en
```

**Server Deployment**
faster-whisper-server (Speaches) via Docker:
```bash
# GPU
docker run -p 8000:8000 ghcr.io/speaches-ai/speaches:latest-cuda

# CPU-only
docker run -p 8000:8000 ghcr.io/speaches-ai/speaches:latest-cpu
```

**UI Settings**
- Transcription Mode toggle: Local / Remote Server
- Server URL input (shown when remote)
- Server Model dropdown (populated from server)
- Connection test button with status indicator

**Checklist Behavior**
- Local mode: Checks if Whisper model is downloaded locally
- Remote mode: Skips local model check, shows server connection status

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
- Session metrics (turns, balance) when diarization enabled
- Note: Cough count removed from UI (audio events now sent to LLM for SOAP context)
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

### Audio Preprocessing (Jan 2025)
Signal conditioning applied before VAD and transcription to improve ASR quality:

**Pipeline Order**
```
Resampler (16kHz) → DC Removal → High-Pass Filter → AGC → VAD → Enhancement → Whisper
```

**DC Offset Removal**
- Single-pole IIR filter removes microphone DC bias
- Alpha: 0.995 (time constant ~200ms at 16kHz)
- Prevents issues with downstream filters

**High-Pass Filter (80Hz)**
- 2nd-order Butterworth biquad filter
- Removes: 50/60Hz power hum, HVAC rumble, desk vibrations
- Uses `biquad` crate with DirectForm2Transposed

**Automatic Gain Control (AGC)**
- Normalizes audio level to consistent RMS (~-20dBFS)
- Handles speakers at varying distances
- Uses `dagc` crate (digital AGC)
- Target RMS: 0.1 (configurable)

**Why No Noise Reduction?**
- Whisper is trained on noisy audio
- Traditional denoising can hurt Whisper accuracy
- We have optional GTCRN enhancement for cases that need it

**Configuration**
- `preprocessing_enabled`: Enable/disable (default: true)
- `preprocessing_highpass_hz`: Filter cutoff (default: 80Hz)
- `preprocessing_agc_target_rms`: AGC target (default: 0.1)

**Performance**
- Latency: <0.5ms total
- CPU: Negligible (O(n) IIR filters)
- Memory: ~1KB state

### Launch Sequence Checklist
- Pre-flight verification system in `checklist.rs`
- Checks: audio devices, models, configuration
- Status types: Pass, Fail, Warning, Skipped
- Extensible for future features (see module docs)

### Test Updates
- All frontend tests passing (335 tests)
- All Rust tests passing (281 tests, including 21 ollama tests)
- Mode component tests: 94 tests (ReadyMode 18, RecordingMode 17, ReviewMode 59)
- Hook tests: 79 tests across 7 hook files (including useSoapNote audio events test)
- Audio quality tests: 16 Rust unit tests, 12 frontend tests
- Audio preprocessing tests: 15+ Rust unit tests (DC, high-pass, AGC)
- SOAP generation tests: 21 Rust tests including 6 new audio event tests
- Fixed clustering.rs bug where max_speakers wasn't enforced
- Added mocks for AuthProvider/useAuth hook in test setup
- Updated mode tests to remove cough display assertions (moved to LLM)

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
  enhancement_enabled: boolean;
  biomarkers_enabled: boolean;
  preprocessing_enabled: boolean;      // Audio preprocessing (default: true)
  preprocessing_highpass_hz: number;   // High-pass filter cutoff (default: 80)
  preprocessing_agc_target_rms: number; // AGC target RMS (default: 0.1)
  ollama_server_url: string;  // e.g., 'http://localhost:11434'
  ollama_model: string;       // e.g., 'qwen3:4b'
  medplum_server_url: string; // e.g., 'http://localhost:8103'
  medplum_client_id: string;  // OAuth client ID from Medplum
  medplum_auto_sync: boolean; // Auto-sync encounters after recording
  // Whisper server (remote transcription)
  whisper_mode: 'local' | 'remote'; // 'local' uses local model, 'remote' uses server
  whisper_server_url: string; // e.g., 'http://192.168.50.149:8000'
  whisper_server_model: string; // e.g., 'large-v3-turbo'
}
```

## File Locations

- **All models**: `~/.transcriptionapp/models/`
  - Whisper: `ggml-{tiny,base,small,medium,large}.bin`
  - Speaker: `speaker_embedding.onnx` (~26MB)
  - Enhancement: `gtcrn_simple.onnx` (~523KB)
  - YAMNet: `yamnet.onnx` (~3MB) - for cough detection
- **Settings**: `~/.transcriptionapp/config.json`
- **Medplum Auth**: `~/.transcriptionapp/medplum_auth.json` - persisted OAuth tokens
- **Activity Logs**: `~/.transcriptionapp/logs/activity.log.*` - daily rotated JSON logs

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

## Timezone Handling

**Principle: Store UTC, Display Local**

All timestamps are stored in UTC and converted to local timezone for display.

**Backend (Rust)**
- All FHIR timestamps use `Utc::now().to_rfc3339()` (RFC3339 format with Z suffix)
- Token expiry uses Unix timestamps (timezone-agnostic)
- Activity logs use explicit UTC via `UtcTime::rfc_3339()`
- Date range queries properly handle day boundaries with `chrono::NaiveDate`

**Frontend (TypeScript)**
- `src/utils.ts` provides centralized date utilities:
  - `formatDateForApi(date)` - Date → YYYY-MM-DD in UTC (for API queries)
  - `formatLocalTime(iso)` - ISO string → local time display (e.g., "2:30 PM")
  - `formatLocalDateTime(iso)` - ISO string → local datetime display
  - `formatLocalDate(iso)` - ISO string → local date display
  - `isSameLocalDay(d1, d2)` - Compare dates in local timezone
  - `isToday(date)` - Check if date is today in local timezone

**Usage Pattern**
```typescript
// Sending to API - use UTC
const dateStr = formatDateForApi(selectedDate);
await invoke('get_encounters', { date: dateStr });

// Displaying to user - use local
<span>{formatLocalTime(encounter.startTime)}</span>
```

## Activity Logging

Structured activity logging for auditing and debugging. PHI-safe by design.

**What IS logged:**
- Session IDs, encounter IDs, segment IDs
- Timestamps and durations
- Event types and outcomes (success/failure)
- File sizes and counts
- Model names and settings
- Error messages (sanitized)

**What is NOT logged:**
- Transcript text
- SOAP note content
- Patient names or identifiers
- Audio content
- Any free-text clinical content

**Architecture**
- `activity_log.rs`: Dual-output logging (console + file)
- Daily rotation via `tracing-appender`
- JSON format for structured analysis
- UTC timestamps with `UtcTime::rfc_3339()`

**Log Events**
| Event | Description |
|-------|-------------|
| `session_start` | Recording session started |
| `session_stop` | Recording session stopped |
| `transcription_segment` | Segment processed (word count only) |
| `soap_generation` | SOAP note generated (no content) |
| `medplum_auth` | Authentication action |
| `encounter_sync` | Encounter synced to Medplum |
| `document_upload` | Document uploaded (size only) |
| `audio_upload` | Audio uploaded (size/duration only) |
| `model_load` | Model loaded |
| `error` | Error occurred (sanitized message) |

## ADRs

See `docs/adr/` for Architecture Decision Records:
- 0001: Use Tauri for desktop app
- 0002: Whisper for transcription
- 0003: VAD-gated processing
- 0004: Ring buffer audio pipeline
- 0005: Session state machine
- 0006: Speaker diarization (online clustering)
- 0007: Biomarker analysis (vitality, stability, cough detection)
- 0008: Medplum EMR integration (OAuth, FHIR resources)
- 0009: Ollama SOAP note generation (JSON output)
- 0010: Audio preprocessing (DC removal, high-pass, AGC)

## Frontend Components

The React frontend is organized into modes and reusable components:

### Mode Components (`src/components/modes/`)
| Component | Purpose |
|-----------|---------|
| `ReadyMode.tsx` | Pre-recording state (checklist, device selection, start button) |
| `RecordingMode.tsx` | Active recording (timer, audio quality, biomarkers, transcript preview) |
| `ReviewMode.tsx` | Post-recording (full transcript, SOAP generation, EMR sync) |

### UI Components (`src/components/`)
| Component | Purpose |
|-----------|---------|
| `Header.tsx` | App title bar, history button, settings toggle |
| `SettingsDrawer.tsx` | Slide-out settings panel with all configuration options |
| `AudioQualitySection.tsx` | Real-time audio level, SNR, clipping display |
| `BiomarkersSection.tsx` | Vitality, stability, session metrics display |
| `ConversationDynamicsSection.tsx` | Turn-taking, overlap, response latency metrics |

### EMR Components (`src/components/`)
| Component | Purpose |
|-----------|---------|
| `AuthProvider.tsx` | React context for Medplum OAuth state |
| `LoginScreen.tsx` | Medplum login button and status |
| `PatientSearch.tsx` | FHIR patient search with autocomplete |
| `EncounterBar.tsx` | Active encounter display with patient info |
| `HistoryWindow.tsx` | Main content for separate history window |
| `HistoryView.tsx` | Encounter list and detail view |
| `Calendar.tsx` | Date picker for history filtering |
| `AudioPlayer.tsx` | Playback controls for recorded audio |

### Shared Types (`src/types/index.ts`)
All TypeScript types that mirror Rust backend structures:
- `SessionState`, `SessionStatus` - Recording state machine
- `TranscriptUpdate` - Real-time transcript data
- `BiomarkerUpdate`, `AudioQualitySnapshot` - Metrics events
- `SoapNote`, `OllamaStatus` - LLM integration
- `AuthState`, `Encounter`, `Patient`, `SyncResult` - Medplum types
- `CheckResult`, `ChecklistResult` - Pre-flight checks

### Utilities (`src/utils.ts`)
Date/time formatting with timezone handling:
- `formatTime(ms)` - Duration formatting (MM:SS or HH:MM:SS)
- `formatDateForApi(date)` - YYYY-MM-DD in UTC
- `formatLocalTime/Date/DateTime(iso)` - Local timezone display
- `isSameLocalDay(d1, d2)`, `isToday(date)` - Date comparisons
- `debounce(fn, delay)` - Debounce utility
- `formatErrorMessage(e)` - Safe error message extraction

### Hooks (`src/hooks/`)
Reusable React hooks for state management:
| Hook | Purpose |
|------|---------|
| `useSessionState` | Recording session state, transcript, biomarkers, audio quality |
| `useChecklist` | Pre-flight checks, model status, download handling |
| `useSoapNote` | SOAP note generation via Ollama |
| `useMedplumSync` | Medplum EMR synchronization |
| `useSettings` | Settings management with pending changes tracking |
| `useDevices` | Audio input device listing and selection |
| `useOllamaConnection` | Ollama server connection status and testing |
| `useWhisperModels` | Whisper model listing, downloading, and testing |
