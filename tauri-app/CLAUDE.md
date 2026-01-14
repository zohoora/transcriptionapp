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
├── listening.rs     # Auto-session detection (VAD + Whisper + LLM greeting check)
├── llm_client.rs    # OpenAI-compatible LLM router client for SOAP note generation
├── ollama.rs        # Re-exports from llm_client.rs (backward compatibility)
├── medplum.rs       # Medplum FHIR client (OAuth, encounters, documents)
├── whisper_server.rs # Remote Whisper server client (faster-whisper)
├── activity_log.rs  # Structured activity logging (PHI-safe)
├── debug_storage.rs # Local debug storage for PHI (development only)
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
| `check_ollama_status` | Check LLM router connection and list models (OpenAI-compatible API) |
| `list_ollama_models` | Get available models from LLM router |
| `generate_soap_note` | Generate SOAP note from transcript via LLM router (legacy) |
| `generate_soap_note_auto_detect` | Generate multi-patient SOAP notes with auto physician/patient detection |
| `medplum_multi_patient_quick_sync` | Sync multi-patient session to Medplum (creates N encounters) |
| `check_whisper_server_status` | Check remote Whisper server connection |
| `list_whisper_server_models` | Get available models from Whisper server |
| `medplum_quick_sync` | Auto-sync transcript/audio to Medplum, returns encounter IDs |
| `medplum_add_soap_to_encounter` | Add SOAP note to existing synced encounter |
| `start_listening` | Start listening mode for auto-session detection |
| `stop_listening` | Stop listening mode |
| `get_listening_status` | Get current listening mode status |

## Key Events (Backend → Frontend)

| Event | Payload |
|-------|---------|
| `session_status` | `{ state, provider, elapsed_ms, is_processing_behind, error_message? }` |
| `transcript_update` | `{ finalized_text, draft_text, segment_count }` |
| `biomarker_update` | `{ cough_count, cough_rate_per_min, turn_count, vitality_session_mean, stability_session_mean, ... }` |
| `audio_quality` | `{ timestamp_ms, peak_db, rms_db, snr_db, clipped_ratio, dropout_count, ... }` |
| `listening_event` | `{ type, duration_ms?, transcript?, confidence?, reason?, ... }` - Auto-session detection events |

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

Integration with OpenAI-compatible LLM router for generating structured SOAP (Subjective, Objective, Assessment, Plan) notes from clinical transcripts.

**Features**
- OpenAI-compatible API (`/v1/chat/completions`, `/v1/models`)
- Configurable LLM router URL, API key, and client ID
- Separate model selection for SOAP generation and fast tasks (greeting detection)
- "Generate SOAP Note" button appears when session is completed
- Collapsible SOAP section with copy functionality
- Connection status indicator in settings
- **Audio events included** (coughs, laughs, sneezes) with confidence scores

**Architecture**
- `llm_client.rs`: Async HTTP client using reqwest for OpenAI-compatible API
- `ollama.rs`: Re-exports from `llm_client.rs` for backward compatibility
- API endpoints used:
  - `GET /v1/models` - List available models
  - `POST /v1/chat/completions` - Generate SOAP note (stream: false)
- Authentication headers:
  - `Authorization: Bearer <api_key>` - API authentication
  - `X-Client-Id: <client_id>` - Client identification (e.g., "clinic-001")
  - `X-Clinic-Task: <task>` - Task type ("soap-generation", "greeting-check", "prewarm")
- **Simplified output**: LLM response displayed as-is (no JSON parsing required)
- Handles channel markers (`<|channel|>` tags) by stripping them
- Exponential backoff retry for transient failures

**Audio Events in SOAP Generation (Jan 2025)**
- YAMNet-detected audio events are passed to the LLM for clinical context
- Events include: Cough, Laughter, Sneeze, Throat clearing, etc.
- Each event includes timestamp and confidence score (converted to percentage)
- Events are NOT displayed in UI (removed) but used for SOAP note generation
- Helps LLM understand patient's condition (e.g., frequent coughing → respiratory issue)

**Prompt Format (Simplified - Jan 2025)**
```
Generate a SOAP note from this clinical transcript.

TRANSCRIPT:
[transcript text]

AUDIO EVENTS DETECTED:
- Cough at 0:30 (confidence: 88%)
- Laughter at 1:05 (confidence: 82%)

Format the note with clear S, O, A, P sections. Be concise and clinical.
Only include information from the transcript. Use "Not documented" for missing sections.
```

Note: The LLM response is displayed as-is without parsing. This simplified approach avoids JSON parsing failures and lets the LLM format the note naturally.

**Configuration**
- `llm_router_url`: LLM router address (default: `http://localhost:4000`)
- `llm_api_key`: API key for authentication
- `llm_client_id`: Client identifier for tracking (e.g., "clinic-001")
- `soap_model`: Model for SOAP generation (e.g., "gpt-4", "claude-3-opus")
- `fast_model`: Model for quick tasks like greeting detection (e.g., "gpt-3.5-turbo")
- Settings persisted in `~/.transcriptionapp/config.json`

**UI Flow**
1. Complete a recording session with transcript
2. SOAP Note section appears below transcript
3. Click "Generate SOAP Note" (requires LLM router connection)
4. Loading spinner during generation (~10-30s depending on model)
5. LLM response displayed as single text block (pre-formatted)
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

### Transcript Length Handling for Long Sessions (Jan 14, 2025)
Added automatic transcript truncation for very long clinical sessions (2+ hours) to prevent LLM context overflow.

**Problem**
Long sessions can produce transcripts exceeding 20,000+ words, which would overflow the LLM's context window (~32K tokens for typical models) and cause SOAP generation failures.

**Solution**
- `prepare_transcript()` validates and optionally truncates transcripts
- `truncate_transcript()` uses a **20/80 split strategy**:
  - Keeps **first 20%** (greeting, patient identification, chief complaint)
  - Keeps **last 80%** (recent discussion, assessment, care plan)
  - Omits middle portion with clear marker

**Limits**
| Constraint | Value | Reason |
|------------|-------|--------|
| `MAX_WORDS_FOR_LLM` | 10,000 words | ~13K tokens, leaves room for system prompt + response |
| `MAX_TRANSCRIPT_SIZE` | 100KB | Prevents memory issues |
| `MIN_TRANSCRIPT_LENGTH` | 50 chars | Ensures meaningful content |
| `MIN_WORD_COUNT` | 5 words | Minimum for SOAP generation |

**Example Truncation**
For a 20,000-word transcript:
```
[First 2,000 words - greeting, chief complaint...]

[... 10,000 words omitted from middle of transcript ...]

[Last 8,000 words - recent discussion, assessment, plan...]
```

**Why 20/80?**
- Beginning: Contains patient greeting, identification, and presenting complaint
- End: Contains most recent assessment and care plan - most relevant for SOAP
- Middle: Often contains examination details that can be summarized from end context

**Files Modified**
- `src-tauri/src/llm_client.rs` - Added `prepare_transcript()`, `truncate_transcript()`, constants
- Added 6 new tests for transcript validation and truncation

**Test Coverage**
- All 19 Rust LLM client tests passing
- All 427 frontend tests passing

### Auto-Detection Toggle UI (Jan 13, 2025)
Added a toggle switch on the main Ready Mode screen to enable/disable auto-session detection.

**Features**
- Toggle visible at the top of the Ready Mode screen
- Settings saved immediately when toggled (persists across app restarts)
- Clean CSS-only switch design matching the app's styling

**Files Modified**
- `src/components/modes/ReadyMode.tsx` - Added toggle UI and `onAutoStartToggle` callback prop
- `src/App.tsx` - Added `handleAutoStartToggle` handler that saves to config
- `src/styles.css` - Added `.auto-detect-toggle` styles

### Simplified SOAP Note Display (Jan 13, 2025)
Removed structured S/O/A/P parsing in favor of displaying the raw LLM output directly.

**Problem**
LLM models often ignore JSON output instructions and return markdown or plain text. Complex parsing logic was fragile and caused failures.

**Solution**
- SOAP note is now stored as a single `content` string field
- LLM response displayed as-is in a pre-formatted text block
- Only cleanup performed: stripping `<|channel|>` tags from multi-channel outputs
- Removed all JSON parsing, markdown extraction, and structured data handling

**Benefits**
1. No more parsing failures - whatever the LLM returns is shown
2. Simpler code - removed ~200 lines of parsing logic
3. LLM can format the note naturally without constraints
4. Works with any model regardless of output format

**Data Structure Changes**
```typescript
// Old structure
interface SoapNote {
  subjective: string;
  objective: string;
  assessment: string;
  plan: string;
  raw_response: string | null;
}

// New simplified structure
interface SoapNote {
  content: string;  // Single text block
  generated_at: string;
  model_used: string;
}

// PatientSoapNote also simplified
interface PatientSoapNote {
  patient_label: string;
  speaker_id: string;
  content: string;  // Direct content, no nested soap object
}
```

**Files Modified**
- `src-tauri/src/llm_client.rs` - Simplified SOAP structures and generation
- `src/types/index.ts` - Updated TypeScript interfaces
- `src/components/modes/ReviewMode.tsx` - Display single content block
- `src/hooks/useMedplumSync.ts` - Updated formatSoapNote function
- `src/App.tsx` - Construct SoapNote from PatientSoapNote
- `src-tauri/src/commands/medplum.rs` - Use content directly

### LLM Router Migration - OpenAI-Compatible API (Jan 12, 2025)
Migrated from Ollama native API to OpenAI-compatible LLM router API for SOAP note generation and greeting detection.

**Why the Change**
- OpenAI-compatible API is industry standard, works with many LLM providers
- Supports authentication headers for multi-tenant deployments
- Enables routing to different models for different tasks (SOAP vs greeting check)
- Better error handling with exponential backoff retry

**API Changes**
| Old (Ollama) | New (OpenAI-compatible) |
|--------------|-------------------------|
| `GET /api/tags` | `GET /v1/models` |
| `POST /api/generate` | `POST /v1/chat/completions` |
| No auth | `Authorization: Bearer <key>` |
| N/A | `X-Client-Id: <client_id>` |
| N/A | `X-Clinic-Task: <task>` |

**Configuration Changes**
| Old Setting | New Setting |
|-------------|-------------|
| `ollama_server_url` | `llm_router_url` |
| `ollama_model` | `soap_model` |
| N/A | `llm_api_key` |
| N/A | `llm_client_id` |
| N/A | `fast_model` |

**Files Modified**
- `src-tauri/src/llm_client.rs` - New OpenAI-compatible client (new file)
- `src-tauri/src/ollama.rs` - Now re-exports from `llm_client.rs`
- `src-tauri/src/config.rs` - New settings fields
- `src-tauri/src/commands/ollama.rs` - Updated for new config
- `src-tauri/src/listening.rs` - Uses fast_model for greeting check
- `src/types/index.ts` - New settings fields, `LLMStatus` type alias
- `src/hooks/useOllamaConnection.ts` - Updated testConnection signature
- `src/hooks/useSettings.ts` - New pending settings fields
- `src/components/SettingsDrawer.tsx` - API Key input, model selects
- Updated all test files with new field names

**Backward Compatibility**
- Command names unchanged (`check_ollama_status`, `list_ollama_models`, etc.)
- TypeScript types have aliases (`OllamaStatus` = `LLMStatus`)
- Hook names unchanged (`useOllamaConnection`)
- Existing configs will need manual update to new field names

**Test Coverage**
- All 430 frontend tests passing
- All Rust tests passing
- Updated tests: `useSettings.test.ts`, `useOllamaConnection.test.ts`, `SettingsDrawer.test.tsx`, `ReviewMode.test.tsx`, `App.test.tsx`

### Multi-Patient SOAP Note Generation (Jan 9, 2025)
Support for multi-patient visits (up to 4 patients) where one recording session produces separate SOAP notes for each patient. The LLM automatically detects the number of patients and identifies the physician from the transcript.

**Key Features**
- **LLM auto-detection**: Identifies patients vs physician from conversation context
- **No manual mapping**: No need to specify which speaker is the physician
- **Dynamic patient count**: Returns 1-4 SOAP notes as needed
- **Single LLM call**: Generates all patient SOAP notes at once for efficiency
- **Backward compatible**: Single patient sessions work exactly as before

**Data Structures**

```typescript
// Frontend types (src/types/index.ts)
interface MultiPatientSoapResult {
  notes: PatientSoapNote[];
  physician_speaker: string | null;  // Which speaker was identified as physician
  generated_at: string;
  model_used: string;
}

interface PatientSoapNote {
  patient_label: string;    // "Patient 1", "Patient 2", etc.
  speaker_id: string;       // "Speaker 2", "Speaker 3", etc.
  soap: SoapNote;           // Standard S/O/A/P structure
}
```

```rust
// Backend types (src-tauri/src/llm_client.rs, re-exported via ollama.rs)
pub struct MultiPatientSoapResult {
    pub notes: Vec<PatientSoapNote>,
    pub physician_speaker: Option<String>,
    pub generated_at: String,
    pub model_used: String,
}

pub struct PatientSoapNote {
    pub patient_label: String,
    pub speaker_id: String,
    pub soap: SoapNote,
}
```

**LLM Prompt Design**
The multi-patient prompt instructs the LLM to:
1. Analyze the conversation to identify who is the PHYSICIAN (asks questions, examines, diagnoses)
2. Identify PATIENTS (describe symptoms, answer questions, receive instructions)
3. NOT assume Speaker 1 is the physician - determine from context
4. Generate one SOAP note per patient with ONLY that patient's information
5. Return structured JSON with physician identification and patient array

**UI Changes**
- **Single Patient (1 note)**: Display unchanged - single S/O/A/P view
- **Multi-Patient (2+ notes)**: Shows patient tabs with speaker ID
  - Tabs: `[Patient 1 (Speaker 1)] [Patient 2 (Speaker 3)]`
  - Each tab displays that patient's S/O/A/P
  - Physician identified at top: "Physician: Speaker 2"
  - Copy button copies active patient's SOAP

**Medplum Sync for Multi-Patient**
- `medplum_multi_patient_quick_sync` creates N placeholder patients and N encounters
- Each patient's SOAP is uploaded to their respective encounter
- Transcript uploaded to all encounters (shared)
- Audio uploaded to first encounter

**Backend Commands**
| Command | Purpose |
|---------|---------|
| `generate_soap_note_auto_detect` | Generate SOAP for 1-4 patients with auto-detection |
| `medplum_multi_patient_quick_sync` | Sync multi-patient session to Medplum |

**Frontend Changes**
- `useSoapNote.ts`: Returns `MultiPatientSoapResult`, added `patientCount`, `isMultiPatient`
- `useMedplumSync.ts`: Added `syncMultiPatientToMedplum()` function
- `useSessionState.ts`: Changed `soapNote` to `soapResult: MultiPatientSoapResult | null`
- `ReviewMode.tsx`: Added patient tabs UI with `activePatient` state
- `App.tsx`: Updated sync flow to handle multi-patient case

**Files Modified**
- `src-tauri/src/llm_client.rs` - Multi-patient types, prompt builder, parser (new file)
- `src-tauri/src/ollama.rs` - Re-exports from llm_client.rs for backward compatibility
- `src-tauri/src/commands/ollama.rs` - New `generate_soap_note_auto_detect` command
- `src-tauri/src/commands/medplum.rs` - New `medplum_multi_patient_quick_sync` command
- `src-tauri/src/lib.rs` - Registered new commands
- `src/types/index.ts` - TypeScript types for multi-patient
- `src/hooks/useSoapNote.ts` - Updated for multi-patient result
- `src/hooks/useMedplumSync.ts` - Added multi-patient sync
- `src/hooks/useSessionState.ts` - Changed `soapNote` to `soapResult`
- `src/components/modes/ReviewMode.tsx` - Patient tabs UI
- `src/styles.css` - `.multi-patient-soap`, `.patient-tabs`, `.patient-tab` styles

**Test Coverage**
- All 429 frontend tests passing
- All 346 Rust tests passing
- Updated tests in: `ReviewMode.test.tsx`, `useSessionState.test.ts`, `useSoapNote.test.ts`

### Auto-Session Detection with Optimistic Recording (Jan 9, 2025)
Automatic session start when a greeting is detected via speech recognition + LLM evaluation.

**How It Works**
1. When `auto_start_enabled` is true, app enters "Listening Mode" when idle
2. VAD monitors for sustained speech (2+ seconds by default)
3. Audio is transcribed via remote Whisper
4. LLM checks if transcript is a greeting (e.g., "Hello", "Good morning", "How are you feeling?")
5. If greeting detected → session auto-starts

**Optimistic Recording (Prevents Audio Loss)**
The LLM greeting check takes ~35 seconds. To prevent losing the conversation start:
1. Recording starts **immediately** when sustained speech is detected
2. Greeting check runs **in parallel** in the background
3. If greeting confirmed → recording continues seamlessly
4. If greeting rejected → recording is discarded

**Flow Diagram**
```
Idle (auto_start=true)
    │
    ▼
┌─────────────┐   speech 2+ sec   ┌──────────────────┐
│  Listening  │ ───────────────▶  │ StartRecording   │
│  (VAD only) │                   │ (optimistic)     │
└─────────────┘                   └──────────────────┘
                                         │
                          ┌──────────────┴──────────────┐
                          │ Greeting check (~35s)       │
                          │ Whisper + LLM               │
                          └──────────────┬──────────────┘
                                         │
                          ┌──────────────┴──────────────┐
                          │                             │
                   GreetingConfirmed              GreetingRejected
                          │                             │
                          ▼                             ▼
                  Recording continues          Recording discarded
```

**Event Types** (`listening_event`)
- `started` - Listening mode started
- `speech_detected` - Sustained speech detected
- `analyzing` - Running Whisper + LLM check
- `start_recording` - Optimistic recording started (before greeting check)
- `greeting_confirmed` - Greeting check passed, recording continues
- `greeting_rejected` - Not a greeting, recording discarded
- `greeting_detected` - Legacy: greeting detected
- `not_greeting` - Legacy: not a greeting
- `error` - Error occurred
- `stopped` - Listening mode stopped

**Backend Architecture**
- `listening.rs`: Audio monitoring, VAD, Whisper transcription, LLM greeting check
- `commands/listening.rs`: Tauri commands and shared state with initial audio buffer
- `llm_client.rs`: `check_greeting()` method for LLM-based greeting detection (uses fast_model)
- `pipeline.rs`: Accepts `initial_audio_buffer` to prepend captured audio

**Frontend Architecture**
- `useAutoDetection` hook: Manages listening mode state and callbacks
- Callbacks: `onStartRecording`, `onGreetingConfirmed`, `onGreetingRejected`
- `isPendingConfirmation` state: True while awaiting greeting check result

**Configuration**
- `auto_start_enabled`: Enable/disable auto-session detection (default: false)
- `greeting_sensitivity`: LLM confidence threshold (default: 0.7)
- `min_speech_duration_ms`: Minimum speech before checking (default: 2000ms)

**Files Modified**
- `src-tauri/src/listening.rs` - Added `StartRecording`, `GreetingConfirmed`, `GreetingRejected` events
- `src-tauri/src/commands/listening.rs` - Added `initial_audio_buffer` to shared state
- `src-tauri/src/commands/session.rs` - Consumes initial audio buffer on session start
- `src-tauri/src/pipeline.rs` - Prepends initial audio buffer at pipeline startup
- `src/types/index.ts` - New event types
- `src/hooks/useAutoDetection.ts` - New callbacks interface, `isPendingConfirmation` state
- `src/App.tsx` - Integrated optimistic recording callbacks

### Auto-Sync to Medplum (Jan 7, 2025)
Automatic synchronization of transcripts and audio to Medplum when recording completes.

**Behavior**
- When a session completes AND user is logged into Medplum AND `medplum_auto_sync` is enabled:
  - Transcript + audio are automatically synced to Medplum
  - Creates a new encounter with placeholder patient
  - Stores encounter IDs for subsequent updates
- When SOAP note is generated:
  - If session was already synced, SOAP is automatically added to the existing encounter
  - No manual sync button click required

**New Backend Command**
- `medplum_add_soap_to_encounter(encounter_fhir_id, soap_note)` - Adds SOAP DocumentReference to existing encounter

**Updated Types**
- `SyncResult` now includes `encounterId` and `encounterFhirId` for tracking
- New `SyncedEncounter` type tracks synced encounter state:
  ```typescript
  interface SyncedEncounter {
    encounterId: string;      // Local UUID
    encounterFhirId: string;  // FHIR server ID
    syncedAt: string;         // ISO timestamp
    hasSoap: boolean;         // Whether SOAP was synced
  }
  ```

**Updated Hook: `useMedplumSync`**
- New state: `syncedEncounter` - tracks the synced encounter for updates
- New state: `isAddingSoap` - true while adding SOAP to encounter
- New function: `addSoapToEncounter(soapNote)` - adds SOAP to existing encounter

**UI Changes**
- Sync button shows progressive states:
  - "Sync to Medplum" (not synced)
  - "Syncing..." (initial sync in progress)
  - "✓ Synced" (transcript synced, no SOAP)
  - "Adding SOAP..." (adding SOAP to encounter)
  - "✓ Synced with SOAP" (fully synced)
- Button disabled after sync (no duplicate syncs)

**Files Modified**
- `src-tauri/src/medplum.rs` - Added fields to `SyncResult`
- `src-tauri/src/commands/medplum.rs` - New command, updated return values
- `src-tauri/src/lib.rs` - Registered new command
- `src/types/index.ts` - New `SyncedEncounter` type
- `src/hooks/useMedplumSync.ts` - Encounter tracking, SOAP update
- `src/App.tsx` - Auto-sync effect, SOAP update integration
- `src/components/modes/ReviewMode.tsx` - Updated sync button states

### Model Indicator UI (Jan 5, 2025)
Shows which transcription mode and model is being used during and after recording:
- **RecordingMode**: Model indicator at bottom of screen (e.g., "🌐 large-v3-turbo" for remote, "💻 large-v3" for local)
- **ReviewMode**: Model shown in session summary bar next to quality badge
- Props: `whisperMode` ('local' | 'remote') and `whisperModel` (string)
- Styles: `.model-indicator` (recording), `.summary-model` (review)

### Remote Whisper Fixes (Jan 5, 2025)
Fixed issues preventing remote Whisper mode from working:

1. **VALID_MODELS list outdated** (`config.rs`):
   - Extended from 5 models to all 17 Whisper models
   - Includes: Standard (8), Large (4), Quantized (2), Distil-Whisper (2)
   - Validation now skipped when `whisper_mode == "remote"`

2. **start_session checked local model in remote mode** (`commands/session.rs`):
   - Fixed to skip local model path validation when `whisper_mode == "remote"`
   - Remote mode uses placeholder path since model lives on server

3. **Anti-hallucination parameters** (`whisper_server.rs`):
   - Added `temperature: 0.0` - Deterministic output
   - Added `no_speech_threshold: 0.8` - Higher threshold (default 0.6)
   - Added `condition_on_previous_text: false` - Prevents repetitive phrases
   - Fixes "Thank you" and "before before before" hallucinations during silence

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
temperature=0.0              # Deterministic output
no_speech_threshold=0.8      # Higher threshold to filter silence
condition_on_previous_text=false  # Prevents repetitive phrases
```

**Anti-hallucination Parameters**
Remote Whisper transcription includes parameters to reduce hallucinations:
- `temperature: 0.0` - Deterministic output, reduces random text
- `no_speech_threshold: 0.8` - Higher than default (0.6) to better filter silence
- `condition_on_previous_text: false` - Prevents repetitive phrases like "Thank you" or "before before"

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

### Test Updates (Jan 12, 2025)
- All frontend tests passing (427 tests across 21 test files)
- All Rust tests passing (including LLM client and multi-patient tests)
- Mode component tests: RecordingMode 23, ReviewMode 46
- Hook tests: useWhisperModels 12, useOllamaConnection 10, useDevices 10, useSettings 12, useSoapNote 16, useChecklist 11, useMedplumSync 9
- Component tests: SettingsDrawer 44, HistoryWindow 36, AuthProvider 8, Header 6, AudioQualitySection 12
- App tests: snapshot 6, a11y 3
- Audio quality tests: 16 Rust unit tests, 12 frontend tests
- Audio preprocessing tests: 15+ Rust unit tests (DC, high-pass, AGC)
- SOAP generation tests: 21 Rust tests including 6 new audio event tests

**Test Fixes Applied (Jan 12, 2025 - LLM Router Migration):**
- Updated `mockSettings` in all test files with new LLM router fields
- Updated `testConnection` signature in `useOllamaConnection.test.ts` (6 params)
- Fixed prewarm duplicate call prevention in `useOllamaConnection.ts`
- Changed "LLM Client ID" label to avoid conflict with Medplum "Client ID"
- Updated App.test.tsx to use "Server Model" instead of "Model"
- Removed unused imports/variables causing TypeScript errors

**Previous Test Fixes:**
- Fixed `vi.useFakeTimers()` isolation issues causing test timeouts
- HistoryWindow: Removed timer-based assertions, added proper `beforeEach`/`afterEach` cleanup
- AuthProvider: Simplified tests to avoid fake timer conflicts with async React operations
- SettingsDrawer: Fixed duplicate element queries by differentiating Whisper/LLM model counts
- useWhisperModels: Switched from `mockResolvedValueOnce` to `mockImplementation` pattern
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
  // LLM Router settings (OpenAI-compatible API)
  llm_router_url: string;     // e.g., 'http://localhost:4000' or 'http://172.16.100.45:4000'
  llm_api_key: string;        // API key for authentication
  llm_client_id: string;      // Client identifier (e.g., 'clinic-001')
  soap_model: string;         // Model for SOAP generation (e.g., 'gpt-4', 'claude-3-opus')
  fast_model: string;         // Model for quick tasks (e.g., 'gpt-3.5-turbo', 'claude-3-haiku')
  // Medplum EMR settings
  medplum_server_url: string; // e.g., 'http://localhost:8103'
  medplum_client_id: string;  // OAuth client ID from Medplum
  medplum_auto_sync: boolean; // Auto-sync encounters after recording
  // Whisper server (remote transcription)
  whisper_mode: 'local' | 'remote'; // 'local' uses local model, 'remote' uses server
  whisper_server_url: string; // e.g., 'http://172.16.100.45:8001'
  whisper_server_model: string; // e.g., 'large-v3-turbo'
  // SOAP generation options
  soap_detail_level: number;  // 1-10, controls verbosity
  soap_format: 'standard' | 'problem_based' | 'systems_based';
  soap_custom_instructions: string;
  // Auto-session detection (listening mode)
  auto_start_enabled: boolean;    // Enable automatic session start on greeting detection
  greeting_sensitivity: number;   // LLM confidence threshold (0.0-1.0, default: 0.7)
  min_speech_duration_ms: number; // Minimum speech duration to trigger check (default: 2000)
  // Debug storage (development only)
  debug_storage_enabled: boolean; // Store PHI locally for debugging (default: true in dev)
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
- **Debug Storage**: `~/.transcriptionapp/debug/<session-id>/` - local PHI storage (development only)

## Debug Storage (Development Only)

**IMPORTANT**: This feature stores PHI (Protected Health Information) locally for debugging purposes. It should be disabled in production by setting `debug_storage_enabled: false` in config.

**Purpose**: Store audio, transcripts, and SOAP notes locally for debugging and analysis during development.

**Storage Location**: `~/.transcriptionapp/debug/<session-id>/`

**Files per session**:
- `audio.wav` - Copy of recorded audio
- `transcript.txt` - Full transcript with speaker labels
- `transcript_segments.json` - Detailed segment data with timestamps
- `soap_note.txt` - Generated SOAP note(s)
- `metadata.json` - Session metadata (timestamps, settings, etc.)

**Configuration**:
- `debug_storage_enabled`: Enable/disable debug storage (default: `true` for development)
- Setting is in `config.rs` and accessible via Settings

**Key Functions** (`debug_storage.rs`):
| Function | Purpose |
|----------|---------|
| `DebugStorage::new(session_id, enabled)` | Create storage instance |
| `add_segment(...)` | Add transcript segment |
| `save_transcript()` | Write transcript files |
| `save_soap_note(content, model)` | Write SOAP note file |
| `finalize(duration_ms)` | Write metadata and finalize |
| `save_soap_note_standalone(...)` | Save SOAP without full DebugStorage instance |
| `list_debug_sessions()` | List all stored sessions |
| `cleanup_old_sessions(keep_count)` | Delete old sessions |

**Integration Points**:
- `stop_session` - Saves transcript and audio when session ends
- `generate_soap_note` - Saves SOAP note with optional session_id
- `generate_soap_note_auto_detect` - Saves multi-patient SOAP notes

## Common Issues

1. **"Model not found"**: Ensure Whisper model file exists at configured path
2. **ONNX tests failing**: Set `ORT_DYLIB_PATH` to ONNX Runtime library
3. **Audio device errors**: Check microphone permissions (macOS: System Settings → Privacy)
4. **OAuth opens new app instance**: Use `pnpm tauri build --debug` instead of `tauri dev`. The single-instance plugin doesn't work in dev mode.
5. **Medplum auth fails**: Verify `medplum_client_id` in `config.rs` matches your Medplum ClientApplication. Delete `~/.transcriptionapp/config.json` to reset to defaults.
6. **Deep links not working**: Ensure app was built (not running via `tauri dev`). Check that `fabricscribe://` URL scheme is registered in Info.plist.

### Testing Best Practices (Vitest)

**Fake Timer Issues:**
- `vi.useFakeTimers()` can cause test isolation problems - subsequent tests may timeout
- Always add `vi.useRealTimers()` in both `beforeEach` and `afterEach` when using fake timers
- Prefer avoiding fake timers when testing React async operations; they conflict with RTL's `waitFor`
- If a test using fake timers times out, it may leave timers in fake mode for subsequent tests

**Mock Patterns:**
- Use `mockImplementation` with command-based routing instead of `mockResolvedValueOnce` chains:
  ```typescript
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'get_settings') return mockSettings;
    if (cmd === 'save_settings') return true;
    return undefined;
  });
  ```
- Avoid `vi.restoreAllMocks()` in `afterEach` - it can interfere with RTL cleanup

**Query Specificity:**
- When multiple elements have the same text, use different test data to distinguish them
- Or use `within()` to scope queries to specific sections
- Or use more specific selectors like `getByRole` with `name` option

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

## Medplum EMR Integration (Dec 2024, Updated Jan 2025)

Integration with Medplum FHIR server for storing encounters, transcripts, SOAP notes, and audio recordings.

**Authentication**
- OAuth 2.0 + PKCE flow via `fabricscribe://oauth/callback` deep link
- Uses `prompt=none` to skip consent screen on subsequent logins (after first consent)
- Session persistence: tokens saved to `~/.transcriptionapp/medplum_auth.json`
- Auto-restore on app startup with automatic token refresh if expired
- Auto-refresh before expiration during session
- Configuration in `config.rs`: `medplum_server_url`, `medplum_client_id`

**Auto-Sync (Jan 2025)**
When `medplum_auto_sync` is enabled and user is authenticated:
1. Session completes → auto-sync transcript + audio to Medplum
2. Encounter created with placeholder patient
3. Encounter IDs stored for subsequent updates
4. SOAP generated → auto-added to existing encounter

This ensures data is preserved even if user doesn't generate a SOAP note.

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
| `medplum_quick_sync` | Auto-sync transcript/audio, returns encounter IDs |
| `medplum_add_soap_to_encounter` | Add SOAP to existing encounter |
| `medplum_sync_encounter` | Manual sync (legacy) |
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
- 0009: LLM SOAP note generation (OpenAI-compatible API, JSON output)
- 0010: Audio preprocessing (DC removal, high-pass, AGC)
- 0011: Auto-session detection (optimistic recording)
- 0012: Multi-patient SOAP generation (LLM auto-detection)
- 0013: LLM Router migration (Ollama → OpenAI-compatible API)

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
- `SoapNote`, `MultiPatientSoapResult`, `PatientSoapNote`, `LLMStatus` (alias: `OllamaStatus`) - LLM integration
- `AuthState`, `Encounter`, `Patient`, `SyncResult`, `SyncedEncounter`, `MultiPatientSyncResult`, `PatientSyncInfo` - Medplum types
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
| `useSoapNote` | SOAP note generation via LLM router |
| `useMedplumSync` | Medplum EMR sync with encounter tracking and SOAP updates |
| `useSettings` | Settings management with pending changes tracking |
| `useDevices` | Audio input device listing and selection |
| `useOllamaConnection` | LLM router connection status and testing (name kept for backward compatibility) |
| `useWhisperModels` | Whisper model listing, downloading, and testing |
| `useAutoDetection` | Auto-session detection via listening mode (VAD + greeting check) |

**`useMedplumSync` Details:**
- `syncToMedplum()` - Initial sync, stores encounter IDs in `syncedEncounter`
- `addSoapToEncounter()` - Adds SOAP to existing encounter
- `syncedEncounter` - Tracks synced encounter for updates
- `isAddingSoap` - True while adding SOAP to encounter
- `resetSyncState()` - Clears sync state for new session

## Current Project Status (Jan 14, 2025)

### What's Working
- **Local transcription**: Full Whisper integration with 17 model options
- **Remote transcription**: faster-whisper-server support with anti-hallucination params
- **SOAP note generation**: OpenAI-compatible LLM router with audio event context
- **Long session support**: Automatic transcript truncation (20/80 split) for 2+ hour sessions
- **Multi-patient SOAP**: LLM auto-detects patients vs physician, generates separate notes
- **Medplum EMR sync**: OAuth + FHIR encounters, documents, audio storage
- **Multi-patient sync**: Creates N patients and N encounters for multi-patient visits
- **Auto-sync**: Automatic sync on session complete, SOAP auto-added to existing encounter
- **Auto-session detection**: Greeting detection via VAD + Whisper + LLM with optimistic recording
- **History window**: Browse past encounters with transcript/SOAP/audio playback
- **Biomarkers**: Vitality, stability, conversation dynamics, audio quality metrics
- **Speaker diarization**: Online clustering for multi-speaker detection
- **Test coverage**: 427 frontend tests, Rust tests all passing

### External Services Configuration
The app connects to external services on the local network:

| Service | Default URL | Purpose |
|---------|-------------|---------|
| Whisper Server | `http://172.16.100.45:8001` | Remote transcription (faster-whisper) |
| LLM Router | `http://172.16.100.45:4000` | SOAP note generation (OpenAI-compatible API) |
| Medplum | `http://172.16.100.45:8103` | EMR/FHIR storage |

**LLM Router Authentication:**
- API Key: Required for authentication (`Authorization: Bearer <key>`)
- Client ID: Identifies the clinic/client (`X-Client-Id` header)
- Task headers: `X-Clinic-Task` indicates operation type

### Known Issues / Areas for Improvement
1. **Hallucination in silence**: Anti-hallucination params added but not fully tested
   - If still occurring, try increasing `no_speech_threshold` to 0.9
   - Or adjust local VAD threshold in settings

2. **Whisper server port**: Currently hardcoded to 8001 in user's config
   - Server was moved from 8000 to 8001 during testing
   - Port is configurable in Settings → Transcription → Remote Server URL

### Quick Start for New AI Coders
```bash
# 1. Build the app
pnpm tauri build --debug

# 2. Bundle ONNX Runtime
./scripts/bundle-ort.sh "src-tauri/target/debug/bundle/macos/Transcription App.app"

# 3. Run the app
open "src-tauri/target/debug/bundle/macos/Transcription App.app"

# Run tests
pnpm test:run          # Frontend (Vitest)
cd src-tauri && cargo test  # Rust
```

### Key Files for Common Tasks

| Task | Files to Modify |
|------|-----------------|
| Add new setting | `config.rs`, `types/index.ts`, `useSettings.ts`, `SettingsDrawer.tsx` |
| Modify transcription | `pipeline.rs`, `whisper_server.rs` (remote), `transcription.rs` (local) |
| Change SOAP prompt | `llm_client.rs` (`build_multi_patient_soap_prompt()`) |
| Modify LLM integration | `llm_client.rs`, `commands/ollama.rs`, `useOllamaConnection.ts` |
| Modify multi-patient SOAP | `llm_client.rs`, `useSoapNote.ts`, `ReviewMode.tsx`, `types/index.ts` |
| Add new biomarker | `biomarkers/mod.rs`, `BiomarkersSection.tsx` |
| Modify UI modes | `components/modes/` (ReadyMode, RecordingMode, ReviewMode) |
| Add Tauri command | `commands/*.rs`, register in `lib.rs` |
| Modify Medplum sync | `commands/medplum.rs`, `useMedplumSync.ts`, `App.tsx` |
| Modify auto-detection | `listening.rs`, `commands/listening.rs`, `useAutoDetection.ts`, `App.tsx` |
