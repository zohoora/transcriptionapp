# Transcription App

A real-time speech-to-text transcription desktop application built with Tauri, React, and Rust. Designed as a clinical ambient scribe for physicians, running as a compact sidebar alongside EMR systems.

## Features

### Core Transcription
- **Real-time streaming transcription** - WebSocket streaming via STT Router with medical-optimized aliases
- **Voice Activity Detection (VAD)** - Silero VAD for smart audio segmentation
- **Speaker diarization** - ONNX-based speaker embeddings with online clustering
- **Speech enhancement** - GTCRN denoising for cleaner audio (~2ms latency)
- **Continuous charting mode** - Records all day, auto-detects encounters, generates SOAP

### Clinical Features
- **SOAP note generation** - AI-powered clinical notes via OpenAI-compatible LLM router
- **Multi-patient SOAP** - Supports up to 4 patients per visit with auto patient/physician detection
- **Medplum EMR integration** - OAuth 2.0 + PKCE, FHIR resources
- **Auto-sync to EMR** - Transcripts and audio automatically synced on session complete
- **Multi-patient sync** - Creates separate encounters for each patient in multi-patient visits
- **Encounter history** - Browse past sessions with calendar view
- **Audio recording** - WAV files synced to EMR
- **Auto-session detection** - Automatically starts recording when a greeting is detected

### Biomarker Analysis
- **Vitality (prosody)** - Pitch variability analysis for affect detection
- **Stability (neurological)** - CPP measurement for vocal control
- **Cough detection** - YAMNet-based audio event classification
- **Conversation dynamics** - Turn-taking, overlap, response latency metrics

### Audio Quality Monitoring
- **Real-time levels** - Peak, RMS, SNR monitoring
- **Clipping detection** - Warns when audio is too loud
- **Noise floor analysis** - Ambient noise level tracking
- **Actionable suggestions** - "Move microphone closer", etc.

### UI/UX
- **Compact sidebar** - 300px width, designed for dual-monitor clinical workflows
- **Light mode** - Clinical-friendly color scheme
- **Collapsible sections** - Transcript, biomarkers, SOAP notes
- **Settings drawer** - Slide-out configuration panel

## Requirements

- Node.js 20+
- Rust 1.70+
- pnpm 10+
- ONNX Runtime (for speaker diarization, enhancement, YAMNet)
- STT Router (required, for speech-to-text via WebSocket streaming)
- LLM Router (required, for SOAP note generation - OpenAI-compatible API)
- Medplum server (optional, for EMR integration)

## Quick Start

```bash
# Install dependencies
pnpm install

# Set up ONNX Runtime
./scripts/setup-ort.sh

# Build the app (recommended over tauri dev)
pnpm tauri build --debug

# Run with ONNX Runtime
ORT_DYLIB_PATH=$(./scripts/setup-ort.sh) \
  "src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"
```

**Note**: Use debug build instead of `tauri dev` for proper deep link and OAuth handling.

## Project Structure

```
tauri-app/
├── src/                          # React frontend
│   ├── App.tsx                   # Main sidebar component
│   ├── components/
│   │   ├── modes/                # UI modes (Ready, Recording, Review, Continuous)
│   │   │   ├── ReadyMode.tsx     # Pre-recording state
│   │   │   ├── RecordingMode.tsx # Active recording
│   │   │   ├── ReviewMode.tsx    # Post-recording review
│   │   │   └── ContinuousMode.tsx # Continuous charting dashboard
│   │   ├── AudioQualitySection.tsx
│   │   ├── BiomarkersSection.tsx
│   │   ├── ConversationDynamicsSection.tsx
│   │   ├── SettingsDrawer.tsx
│   │   ├── Header.tsx
│   │   ├── AuthProvider.tsx      # Medplum OAuth context
│   │   ├── LoginScreen.tsx
│   │   ├── PatientSearch.tsx
│   │   ├── EncounterBar.tsx
│   │   ├── HistoryView.tsx       # Encounter history
│   │   ├── HistoryWindow.tsx     # Separate history window
│   │   ├── Calendar.tsx
│   │   ├── AudioPlayer.tsx
│   │   ├── SpeakerEnrollment.tsx # Speaker voice enrollment
│   │   ├── ClinicalChat.tsx      # Clinical assistant chat
│   │   ├── ImageSuggestions.tsx   # MIIS medical illustrations
│   │   ├── PatientPulse.tsx      # Glanceable biomarker summary
│   │   ├── PatientVoiceMonitor.tsx # Patient voice metric trending
│   │   └── SyncStatusBar.tsx     # EMR sync status
│   ├── types/index.ts            # Shared TypeScript types
│   ├── utils.ts                  # Date/time utilities
│   ├── ErrorBoundary.tsx
│   └── test/                     # Test mocks and utilities
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs                # Tauri plugin registration
│   │   ├── main.rs               # Entry point
│   │   ├── commands/             # IPC command handlers
│   │   │   ├── mod.rs            # Re-exports, CommandError, PipelineState
│   │   │   ├── session.rs        # Recording lifecycle + auto-end
│   │   │   ├── settings.rs       # get/set settings
│   │   │   ├── audio.rs          # Device enumeration
│   │   │   ├── models.rs         # Model downloads
│   │   │   ├── ollama.rs         # LLM router connection
│   │   │   ├── medplum.rs        # EMR sync commands
│   │   │   ├── listening.rs      # Auto-session detection
│   │   │   ├── speaker_profiles.rs # Speaker enrollment CRUD
│   │   │   ├── clinical_chat.rs  # Clinical assistant chat
│   │   │   ├── miis.rs           # Medical illustration proxy
│   │   │   ├── screenshot.rs     # Screen capture
│   │   │   ├── continuous.rs     # Continuous charting mode
│   │   │   ├── archive.rs        # Local session history
│   │   │   ├── whisper_server.rs # STT Router status
│   │   │   └── permissions.rs    # Microphone permissions
│   │   ├── session.rs            # Recording state machine
│   │   ├── pipeline.rs           # Audio processing pipeline
│   │   ├── audio.rs              # Audio capture, resampling
│   │   ├── vad.rs                # Voice Activity Detection
│   │   ├── transcription.rs      # Segment/utterance types
│   │   ├── config.rs             # Settings persistence
│   │   ├── models.rs             # Model download management
│   │   ├── checklist.rs          # Pre-flight checks
│   │   ├── listening.rs          # Auto-session detection (VAD + greeting check)
│   │   ├── llm_client.rs         # OpenAI-compatible LLM client
│   │   ├── ollama.rs             # Re-exports from llm_client.rs (backward compat)
│   │   ├── medplum.rs            # Medplum FHIR client
│   │   ├── continuous_mode.rs    # Continuous charting mode (end-of-day)
│   │   ├── local_archive.rs      # Local session storage
│   │   ├── speaker_profiles.rs   # Speaker enrollment storage
│   │   ├── screenshot.rs         # Screen capture (in-memory JPEG)
│   │   ├── whisper_server.rs     # STT Router client
│   │   ├── debug_storage.rs      # Debug storage (dev only)
│   │   ├── permissions.rs        # macOS permission checks
│   │   ├── activity_log.rs       # Structured logging
│   │   ├── preprocessing.rs      # Audio preprocessing (DC removal, high-pass, AGC)
│   │   ├── diarization/          # Speaker detection
│   │   │   ├── mod.rs            # Module exports
│   │   │   ├── provider.rs       # ONNX embedding extraction
│   │   │   ├── embedding.rs      # Embedding utilities
│   │   │   ├── clustering.rs     # Online speaker clustering
│   │   │   ├── config.rs         # Diarization settings
│   │   │   └── mel.rs            # Mel spectrogram
│   │   ├── enhancement/          # Speech denoising
│   │   │   ├── mod.rs
│   │   │   └── provider.rs       # GTCRN ONNX model
│   │   ├── biomarkers/           # Vocal biomarker analysis
│   │   │   ├── mod.rs            # Types and exports
│   │   │   ├── config.rs         # Biomarker settings
│   │   │   ├── thread.rs         # Sidecar processing thread
│   │   │   ├── audio_quality.rs  # Real-time quality metrics
│   │   │   ├── yamnet/           # Cough detection
│   │   │   ├── voice_metrics/    # Vitality, stability
│   │   │   │   ├── vitality.rs   # F0 pitch analysis
│   │   │   │   └── stability.rs  # CPP cepstral analysis
│   │   │   └── session_metrics/  # Turn-taking stats
│   │   └── mcp/                  # MCP server on port 7101
│   ├── benches/                  # Performance benchmarks
│   └── Cargo.toml
├── docs/
│   └── adr/                      # Architecture Decision Records
├── e2e/                          # End-to-end tests (WebDriver)
├── tests/visual/                 # Visual regression tests (Playwright)
├── CLAUDE.md                     # AI coder context
├── CONTRIBUTING.md               # Contribution guidelines
└── README.md                     # This file
```

## Testing

### Frontend Tests

```bash
pnpm test              # Watch mode
pnpm test:run          # Run once
pnpm test:coverage     # With coverage
pnpm visual:test       # Visual regression (Playwright)
pnpm mutation:test     # Mutation testing (Stryker)
```

### Rust Tests

```bash
cd src-tauri

# Unit tests (needs ORT_DYLIB_PATH for ONNX tests)
ORT_DYLIB_PATH=$(../scripts/setup-ort.sh) cargo test

# Stress tests
cargo test --release stress_test

# Benchmarks
cargo bench

# Fuzz testing (nightly)
cargo +nightly fuzz run fuzz_vad_config
```

### E2E Tests

```bash
pnpm tauri build       # Build first
cargo install tauri-driver
pnpm e2e
```

### Soak Tests

```bash
pnpm soak:quick        # 1 minute
pnpm soak:1h           # 1 hour
pnpm soak:test         # Interactive
```

## Test Coverage

| Category | Framework | Count |
|----------|-----------|-------|
| Unit Tests (Frontend) | Vitest | 387 tests |
| Unit Tests (Rust) | cargo test | 421 tests |
| E2E Integration (Rust) | cargo test (ignored) | 10 tests |
| Snapshot Tests | Vitest | 7 snapshots |
| Accessibility Tests | vitest-axe | 12 tests |
| Contract Tests | Vitest | 24 tests |
| Property-based Tests | proptest | 17 tests |
| Stress Tests | cargo test | 11 tests |
| Pipeline Integration | cargo test | 10 tests |
| Visual Regression | Playwright | 15+ tests |
| E2E Tests | WebDriverIO | 20+ tests |
| Soak Tests | cargo test | 5 tests |

## Configuration

Settings are stored in `~/.transcriptionapp/config.json`:

```json
{
  "language": "en",
  "input_device_id": null,
  "diarization_enabled": false,
  "max_speakers": 10,
  "whisper_server_url": "http://10.241.15.154:8001",
  "stt_alias": "medical-streaming",
  "stt_postprocess": true,
  "llm_router_url": "",
  "llm_api_key": "",
  "llm_client_id": "ai-scribe",
  "soap_model": "soap-model-fast",
  "soap_model_fast": "soap-model-fast",
  "fast_model": "fast-model",
  "medplum_server_url": "",
  "medplum_client_id": "af1464aa-e00c-4940-a32e-18d878b7911c",
  "medplum_auto_sync": true,
  "auto_start_enabled": false,
  "greeting_sensitivity": 0.7,
  "min_speech_duration_ms": 2000,
  "charting_mode": "session"
}
```

## File Locations

| File | Location |
|------|----------|
| All models | `~/.transcriptionapp/models/` |
| Settings | `~/.transcriptionapp/config.json` |
| Speaker profiles | `~/.transcriptionapp/speaker_profiles.json` |
| Medplum auth | `~/.transcriptionapp/medplum_auth.json` |
| Session archive | `~/.transcriptionapp/archive/` |
| Activity logs | `~/.transcriptionapp/logs/activity.log.*` |
| Debug storage | `~/.transcriptionapp/debug/` |

## Architecture

See [docs/adr/](./docs/adr/) for Architecture Decision Records.

## Documentation

- **[CLAUDE.md](./CLAUDE.md)** - Comprehensive context for AI coders
- **[CONTRIBUTING.md](./CONTRIBUTING.md)** - Development guidelines
- **[docs/adr/](./docs/adr/)** - Architecture decisions

## Common Issues

1. **STT Router unreachable**: Verify `whisper_server_url` points to STT Router, check with `curl http://<ip>:8001/health`
2. **ONNX tests failing**: Set `ORT_DYLIB_PATH` to ONNX Runtime library
3. **Audio device errors**: Check microphone permissions (macOS: System Settings > Privacy)
4. **OAuth opens new window**: Use `pnpm tauri build --debug` instead of `tauri dev`
5. **Medplum auth fails**: Verify `medplum_client_id` matches your Medplum ClientApplication
6. **E2E tests failing**: Ensure STT Router and LLM Router are running and API key is in config

## License

MIT
