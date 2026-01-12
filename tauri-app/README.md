# Transcription App

A real-time speech-to-text transcription desktop application built with Tauri, React, and Rust. Designed as a clinical ambient scribe for physicians, running as a compact sidebar alongside EMR systems.

## Features

### Core Transcription
- **Real-time Whisper transcription** - Local on-device inference using whisper-rs
- **Voice Activity Detection (VAD)** - Silero VAD for smart audio segmentation
- **Speaker diarization** - ONNX-based speaker embeddings with online clustering
- **Speech enhancement** - GTCRN denoising for cleaner audio (~2ms latency)
- **Multiple Whisper models** - tiny, base, small, medium, large

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
- ONNX Runtime (for speaker diarization, enhancement, emotion, YAMNet)
- LLM Router (optional, for SOAP note generation - OpenAI-compatible API)
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
│   │   ├── modes/                # UI modes (Ready, Recording, Review)
│   │   │   ├── ReadyMode.tsx     # Pre-recording state
│   │   │   ├── RecordingMode.tsx # Active recording
│   │   │   └── ReviewMode.tsx    # Post-recording review
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
│   │   └── AudioPlayer.tsx
│   ├── types/index.ts            # Shared TypeScript types
│   ├── utils.ts                  # Date/time utilities
│   ├── ErrorBoundary.tsx
│   └── test/                     # Test mocks and utilities
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs                # Tauri plugin registration
│   │   ├── main.rs               # Entry point
│   │   ├── commands.rs           # IPC command handlers
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
│   │   ├── activity_log.rs       # Structured logging
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
│   │   ├── emotion/              # Emotion detection
│   │   │   ├── mod.rs
│   │   │   └── provider.rs       # wav2small ONNX model
│   │   └── biomarkers/           # Vocal biomarker analysis
│   │       ├── mod.rs            # Types and exports
│   │       ├── config.rs         # Biomarker settings
│   │       ├── thread.rs         # Sidecar processing thread
│   │       ├── audio_quality.rs  # Real-time quality metrics
│   │       ├── yamnet/           # Cough detection
│   │       ├── voice_metrics/    # Vitality, stability
│   │       │   ├── vitality.rs   # F0 pitch analysis
│   │       │   └── stability.rs  # CPP cepstral analysis
│   │       └── session_metrics/  # Turn-taking stats
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
| Unit Tests (Frontend) | Vitest | 430 tests |
| Unit Tests (Rust) | cargo test | 346 tests |
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
  "whisper_model": "small",
  "language": "en",
  "input_device_id": null,
  "diarization_enabled": true,
  "max_speakers": 2,
  "llm_router_url": "http://localhost:4000",
  "llm_api_key": "",
  "llm_client_id": "clinic-001",
  "soap_model": "gpt-4",
  "fast_model": "gpt-3.5-turbo",
  "medplum_server_url": "http://localhost:8103",
  "medplum_client_id": "your-client-id",
  "medplum_auto_sync": true,
  "auto_start_enabled": false,
  "greeting_sensitivity": 0.7,
  "min_speech_duration_ms": 2000
}
```

## File Locations

| File | Location |
|------|----------|
| All models | `~/.transcriptionapp/models/` |
| Settings | `~/.transcriptionapp/config.json` |
| Medplum auth | `~/.transcriptionapp/medplum_auth.json` |
| Activity logs | `~/.transcriptionapp/logs/activity.log.*` |

## Architecture

See [docs/adr/](./docs/adr/) for Architecture Decision Records.

## Documentation

- **[CLAUDE.md](./CLAUDE.md)** - Comprehensive context for AI coders
- **[CONTRIBUTING.md](./CONTRIBUTING.md)** - Development guidelines
- **[docs/adr/](./docs/adr/)** - Architecture decisions

## Common Issues

1. **Model not found**: Ensure Whisper model exists at `~/.transcriptionapp/models/`
2. **ONNX tests failing**: Set `ORT_DYLIB_PATH` to ONNX Runtime library
3. **Audio device errors**: Check microphone permissions (macOS: System Settings > Privacy)
4. **OAuth opens new window**: Use `pnpm tauri build --debug` instead of `tauri dev`
5. **Medplum auth fails**: Verify `medplum_client_id` matches your Medplum ClientApplication

## License

MIT
