# Claude Code Context

Clinical ambient scribe for physicians - real-time speech-to-text transcription desktop app.

## Tech Stack

- **Frontend**: React + TypeScript + Vite
- **Backend**: Rust + Tauri v2
- **Transcription**: Whisper (local via whisper-rs, or remote via faster-whisper-server)
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
│   ├── clinical_chat.rs  # Clinical assistant chat proxy
│   └── ...
├── session.rs         # Recording state machine
├── audio.rs           # Audio capture, resampling
├── vad.rs             # Voice Activity Detection
├── pipeline.rs        # Processing pipeline + silence tracking
├── config.rs          # Settings persistence
├── llm_client.rs      # OpenAI-compatible LLM client
├── medplum.rs         # FHIR client (OAuth, encounters)
├── whisper_server.rs  # Remote Whisper client
├── listening.rs       # Auto-session detection
├── speaker_profiles.rs # Speaker enrollment storage
├── local_archive.rs   # Local session storage
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

# Tests
pnpm test:run                    # Frontend (Vitest)
cd src-tauri && cargo test       # Rust
```

**Why not `tauri dev`?** Deep links and single-instance plugin don't work in dev mode. OAuth callbacks open new instances instead of routing to existing app.

## Key Files for Common Tasks

| Task | Files to Modify |
|------|-----------------|
| Add new setting | `config.rs`, `types/index.ts`, `useSettings.ts`, `SettingsDrawer.tsx` |
| Modify transcription | `pipeline.rs`, `whisper_server.rs` (remote), `transcription.rs` (local) |
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

## IPC Commands

| Command | Purpose |
|---------|---------|
| `start_session` / `stop_session` / `reset_session` | Recording lifecycle |
| `run_checklist` | Pre-flight checks |
| `list_input_devices` | Available microphones |
| `get_settings` / `set_settings` | Configuration |
| `get_whisper_models` / `download_whisper_model_by_id` | Model management |
| `check_ollama_status` / `list_ollama_models` | LLM router connection |
| `generate_soap_note_auto_detect` | Multi-patient SOAP generation |
| `medplum_quick_sync` / `medplum_add_soap_to_encounter` | EMR sync |
| `start_listening` / `stop_listening` | Auto-session detection |
| `list_speaker_profiles` / `create_speaker_profile` | Speaker enrollment management |
| `update_speaker_profile` / `delete_speaker_profile` | Speaker profile CRUD |
| `reenroll_speaker_profile` | Re-record voice sample for existing profile |
| `reset_silence_timer` | Cancel auto-end countdown |
| `clinical_chat_send` | Send message to clinical assistant LLM |

## Events (Backend → Frontend)

| Event | Purpose |
|-------|---------|
| `session_status` | Recording state changes |
| `transcript_update` | Real-time transcript |
| `biomarker_update` | Vitality, stability, session metrics |
| `audio_quality` | Level, SNR, clipping metrics |
| `listening_event` | Auto-detection status (includes `speaker_not_verified`) |
| `silence_warning` | Auto-end countdown (silence_ms, remaining_ms) |
| `session_auto_end` | Session auto-ended due to silence |

## Session States

```
Idle → Preparing → Recording → Stopping → Completed
  ↑                                           │
  └──────────── Reset / Error ←───────────────┘
```

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
- **Local**: 17 Whisper models (tiny → large-v3-turbo, quantized, distil)
- **Remote**: faster-whisper-server with anti-hallucination params
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
- SOAP auto-added to existing encounter

### Biomarkers
- **Vitality**: Pitch variability (F0 std dev) - detects flat affect
- **Stability**: CPP (Cepstral Peak Prominence) - vocal fold regularity
- **Cough Detection**: YAMNet ONNX model
- **Conversation Dynamics**: Turn-taking, overlap, response latency, engagement score

**Interpretable Metrics** (Insights tab in ReviewMode):
| Metric | Unit | Good | Moderate | Concerning |
|--------|------|------|----------|------------|
| Vitality | Hz | ≥30 (Normal) | 15-30 (Reduced) | <15 (Low) |
| Stability | dB | ≥8 (Good) | 5-8 (Moderate) | <5 (Unstable) |
| Engagement | 0-100 | ≥70 (Good) | 40-70 (Moderate) | <40 (Low) |
| Response Time | ms | ≤500 (Quick) | 500-1500 (Moderate) | >1500 (Slow) |

Helper functions: `getVitalityStatus()`, `getStabilityStatus()`, `getEngagementStatus()`, `getResponseTimeStatus()` in `types/index.ts`

### Speaker Enrollment
- Train diarization to recognize known speakers by voice
- Profiles include: name, role (Physician/PA/RN/MA/Patient/Other), description
- 256-dim ECAPA-TDNN voice embeddings stored in `speaker_profiles.json`
- Enrolled speakers loaded at session start, matched with higher threshold (0.6 vs 0.3)
- Speaker context injected into SOAP prompts for better clinical attribution

**Enrollment Flow**:
1. Settings → Speaker Profiles → Add
2. Enter name, role, description
3. Record 5-15 second voice sample
4. Save profile (embedding extracted automatically)

**Recognition Priority**: Enrolled speakers checked first → fall back to auto-clustering if no match

### Clinical Assistant Chat
Real-time chat with an LLM during recording sessions for quick medical lookups.

**Features**:
- Collapsible chat window in RecordingMode
- Markdown rendering (bold, italic, code, lists, headers)
- Shows when web search tools were used

**Architecture**:
- Frontend: `useClinicalChat.ts` hook, `ClinicalChat.tsx` component
- Backend: `commands/clinical_chat.rs` - proxies HTTP through Rust (bypasses browser CSP)
- Uses `clinical-assistant` model alias on LLM router

**Router Requirements**:
The LLM router must handle tool execution for the `clinical-assistant` alias. If the model returns a tool call (e.g., `{"toolcall": {"name": "toolsearch", ...}}`), the router must:
1. Execute the tool (web search, etc.)
2. Feed results back to the model
3. Return the final response

Without router-side tool execution, raw tool call JSON will be displayed instead of results.

### Auto-End Silence Detection
Automatically ends recording sessions after prolonged silence.

**Configuration**: `auto_end_enabled` setting + `auto_end_silence_ms` (default: 120000ms = 2 minutes)

**Flow**:
1. VAD tracks continuous silence during recording
2. After threshold reached, emits `SilenceWarning` with countdown
3. User can cancel via "Keep Recording" button (calls `reset_silence_timer`)
4. If not cancelled, session auto-stops gracefully

**Files**: `pipeline.rs` (silence tracking), `commands/session.rs` (reset command), `useSessionState.ts` (UI)

### MCP Server
Port 7101, JSON-RPC 2.0. Tools: `agent_identity`, `health_check`, `get_status`, `get_logs`

## Settings Schema

```typescript
interface Settings {
  // Transcription
  whisper_model: string;
  whisper_mode: 'local' | 'remote';
  whisper_server_url: string;
  whisper_server_model: string;
  language: string;

  // Audio
  input_device_id: string | null;
  vad_threshold: number;
  diarization_enabled: boolean;
  max_speakers: number;  // 2-10
  enhancement_enabled: boolean;
  biomarkers_enabled: boolean;
  preprocessing_enabled: boolean;

  // LLM Router
  llm_router_url: string;
  llm_api_key: string;
  llm_client_id: string;
  soap_model: string;       // Long sessions (≥5K words)
  soap_model_fast: string;  // Short sessions (<5K words)
  fast_model: string;       // Greeting detection

  // Medplum
  medplum_server_url: string;
  medplum_client_id: string;
  medplum_auto_sync: boolean;

  // Auto-detection
  auto_start_enabled: boolean;
  auto_start_require_enrolled: boolean;  // Require enrolled speaker for auto-start
  auto_start_required_role: string | null; // Role filter (e.g., "physician")
  auto_end_enabled: boolean;
  auto_end_silence_ms: number;   // Default: 120000 (2 minutes)
  greeting_sensitivity: number;  // 0.0-1.0
  min_speech_duration_ms: number;

  // SOAP Generation
  soap_detail_level: number;       // 1-10, controls verbosity (persisted across sessions)
  soap_format: 'problem_based' | 'comprehensive';  // Organization style
  soap_custom_instructions: string; // Additional prompt instructions

  // Debug
  debug_storage_enabled: boolean;  // PHI storage for dev only
}
```

## File Locations

| Path | Contents |
|------|----------|
| `~/.transcriptionapp/models/` | Whisper, speaker embedding, enhancement, YAMNet models |
| `~/.transcriptionapp/config.json` | Settings |
| `~/.transcriptionapp/speaker_profiles.json` | Enrolled speaker voice profiles |
| `~/.transcriptionapp/medplum_auth.json` | OAuth tokens |
| `~/.transcriptionapp/logs/` | Activity logs (daily rotation, PHI-safe) |
| `~/.transcriptionapp/debug/` | Debug storage (dev only) |

## External Services

| Service | Default URL | Purpose |
|---------|-------------|---------|
| Whisper Server | `http://10.241.15.154:8001` | Remote transcription |
| LLM Router | `http://10.241.15.154:8080` | SOAP generation |
| Medplum | `http://10.241.15.154:8103` | EMR/FHIR |

## Frontend Structure

**Mode Components** (`src/components/modes/`):
- `ReadyMode.tsx` - Pre-recording (checklist, device selection)
- `RecordingMode.tsx` - Active recording (timer, quality, transcript preview)
- `ReviewMode.tsx` - Post-recording (transcript, SOAP, EMR sync)

**Key Hooks** (`src/hooks/`):
- `useSessionState` - Recording state, transcript, biomarkers
- `useSoapNote` - SOAP generation
- `useMedplumSync` - EMR sync with encounter tracking
- `useSettings` - Configuration management
- `useAutoDetection` - Listening mode
- `useSpeakerProfiles` - Speaker enrollment CRUD operations
- `useClinicalChat` - Clinical assistant chat during recording

**Speaker Enrollment** (`src/components/`):
- `SpeakerEnrollment.tsx` - Profile list, enrollment form, audio recording

**Types**: `src/types/index.ts` - All TypeScript interfaces mirroring Rust structs, plus:
- `BIOMARKER_THRESHOLDS` - Clinical thresholds for biomarker interpretation
- `getVitalityStatus()`, `getStabilityStatus()`, `getEngagementStatus()`, `getResponseTimeStatus()` - Convert raw values to interpretable labels

## Common Issues

| Problem | Solution |
|---------|----------|
| "Model not found" | Check `~/.transcriptionapp/models/` for Whisper model |
| ONNX tests failing | Set `ORT_DYLIB_PATH` environment variable |
| Audio device errors | Check macOS microphone permissions |
| OAuth opens new instance | Use `pnpm tauri build --debug`, not `tauri dev` |
| Deep links not working | Ensure app was built and `fabricscribe://` scheme registered |
| Clinical chat shows raw JSON | Router must execute tools for `clinical-assistant` alias |
| Speaker verification fails | Ensure profiles exist and speaker model at `~/.transcriptionapp/models/ecapa_tdnn.onnx` |
| Auto-end too aggressive | Increase `auto_end_silence_ms` or disable `auto_end_enabled` |
| SOAP not copying to clipboard | Check Tauri clipboard plugin permissions |

## Testing Best Practices

- Avoid `vi.useFakeTimers()` with React async - conflicts with RTL's `waitFor`
- Use `mockImplementation` with command routing instead of `mockResolvedValueOnce` chains
- Always clean up timers in `beforeEach`/`afterEach`

## Adding New Features

1. **Config**: Add field to `config.rs`, `types/index.ts`, `useSettings.ts`, `SettingsDrawer.tsx`
2. **Model**: Add URL/download in `models.rs`, check in `checklist.rs`
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
