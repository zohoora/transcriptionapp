# Transcription App

A clinical ambient scribe for physicians. Real-time speech-to-text transcription desktop app with automated encounter detection, SOAP note generation, and multi-room clinic deployment. Includes an iOS mobile app for house call recordings with server-side processing.

## Status

**Production-Ready** - All core features implemented and tested.

## Features

### Core Transcription
- **STT Router Streaming** - Real-time transcription via WebSocket streaming through STT Router with medical-optimized aliases
- **Real-time Updates** - Finalized transcript within 1-4 seconds of each utterance
- **VAD-Gated** - Voice Activity Detection prevents hallucinations during silence
- **Speaker Diarization** - Identifies up to 10 different speakers in conversation
- **Speech Enhancement** - GTCRN denoising for cleaner audio (~2ms latency)

### Clinical Features
- **SOAP Note Generation** - AI-powered clinical notes via OpenAI-compatible LLM router
- **Multi-Patient SOAP** - Supports up to 4 patients per visit with auto-detection
- **Audio Event Context** - Coughs, laughs, sneezes included in SOAP generation
- **Clinical Assistant Chat** - Real-time LLM chat during recording via `clinical-assistant` model alias
- **Medplum EMR Integration** - OAuth 2.0 + PKCE, FHIR resources
- **Auto-Sync to EMR** - Transcripts and audio automatically synced on session complete
- **Encounter History** - Browse past sessions with calendar view
- **Auto-Session Detection** - Automatically starts recording when greeting detected
- **Speaker Enrollment** - Voice profiles with ECAPA-TDNN embeddings for speaker-verified auto-start
- **Auto-End Silence Detection** - Automatically ends recording after configurable silence threshold
- **Continuous Charting Mode** - Records all day, auto-detects encounters, generates SOAP at end of day
- **Vision-Based Patient Name Extraction** - Screenshots + vision LLM extract patient names from on-screen EHR
- **MIIS Image Suggestions** - Medical illustration images suggested from transcript concepts
- **MCP Server** - JSON-RPC 2.0 server on port 7101 for external tool integration
- **Mobile House Call Recording** - iOS app records offline, auto-uploads when on network, server processes via shared Rust pipeline

### Biomarker Analysis
- **PatientPulse Display** - Glanceable "check engine light" for patient voice metrics (hidden/normal/alert states)
- **Vitality** - Pitch variability for affect detection (depression/PTSD indicator)
- **Stability** - CPP measurement for vocal control (Parkinson's indicator)
- **Cough Detection** - YAMNet-based audio event classification
- **Conversation Dynamics** - Turn-taking, overlap, response latency, engagement score

### Audio Quality Monitoring
- **Real-time Levels** - Peak, RMS, SNR monitoring
- **Clipping Detection** - Warns when audio is too loud
- **Noise Floor Analysis** - Ambient noise level tracking
- **Actionable Suggestions** - Context-aware feedback

> **Note on Network Features**: The transcription pipeline requires the STT Router (WebSocket streaming to Whisper backend). SOAP generation (LLM Router) and EMR sync (Medplum) also require network access.

## Quick Start

### Prerequisites

- Rust 1.70+
- Node.js 20+
- pnpm 10+
- ONNX Runtime (for diarization, enhancement, YAMNet)

### Run Desktop App

```bash
cd tauri-app
pnpm install

# Set up ONNX Runtime
./scripts/setup-ort.sh

# Build debug app (required for OAuth deep links)
pnpm tauri build --debug

# Run with ONNX Runtime
ORT_DYLIB_PATH=$(./scripts/setup-ort.sh) \
  "src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"
```

> **Note**: Use the debug build instead of `pnpm tauri dev` for proper deep link and single-instance handling.

## Documentation

- **[tauri-app/CLAUDE.md](tauri-app/CLAUDE.md)** - Comprehensive AI coder context (desktop app)
- **[profile-service/CLAUDE.md](profile-service/CLAUDE.md)** - Profile service architecture and patterns
- **[docs/superpowers/specs/2026-04-13-mobile-app-v1-design.md](docs/superpowers/specs/2026-04-13-mobile-app-v1-design.md)** - Mobile app design spec
- **[tauri-app/CONTRIBUTING.md](tauri-app/CONTRIBUTING.md)** - Development guidelines
- **[tauri-app/README.md](tauri-app/README.md)** - Desktop app documentation
- **[tauri-app/docs/adr/](tauri-app/docs/adr/)** - Architecture Decision Records

## Architecture

```
┌─────────────────┐         ┌───────────────────────────────────────────────┐
│   iOS App        │         │  MacBook Server                               │
│   (SwiftUI)      │  HTTP   │                                               │
│  Record audio ───┼────────►│  Profile Service (:8090)                      │
│  Upload AAC   ───┼────────►│    └─ Mobile job tracking + audio storage     │
│  Poll status  ───┼────────►│                                               │
└─────────────────┘         │  Processing CLI (process_mobile)              │
                            │    └─ Shares desktop Rust modules (STT, SOAP) │
┌─────────────────────────────────────────────────────────────────────────┐ │
│                         Desktop App (Tauri)                               │ │
│  ┌─────────────────┐      IPC       ┌─────────────────────────────────┐ │ │
│  │   React/TS UI   │◄──────────────►│          Rust Backend           │ │ │
│  │  - Sidebar      │    Events      │  - Session State Machine        │ │ │
│  │  - Settings     │                │  - Audio Pipeline               │ │ │
│  │  - Transcript   │                │  - STT Router + Diarization     │ │ │
│  │  - SOAP Notes   │                │  - Biomarker Analysis           │ │ │
│  │  - EMR Sync     │                │  - LLM Router + Medplum         │ │ │
│  │  - Clinical Chat│                │  - Continuous Mode              │ │ │
│  │  - Continuous   │                │  - Speaker Profiles             │ │ │
│  └─────────────────┘                │  - MCP Server (port 7101)       │ │ │
│                                     └─────────────────────────────────┘ │ │
└─────────────────────────────────────────────────────────────────────────┘ │
                            │  STT Router (:8001)  |  LLM Router (:8080)    │
                            └───────────────────────────────────────────────┘
```

## Technology Stack

| Component | Technology |
|-----------|------------|
| Desktop Shell | Tauri 2.x |
| UI | React + TypeScript + Vite |
| Audio Capture | cpal |
| Ring Buffer | ringbuf |
| Resampling | rubato |
| VAD | voice_activity_detector (Silero) |
| Transcription | STT Router (WebSocket streaming to Whisper backend) |
| Speaker Diarization | ONNX Runtime + WeSpeaker |
| Speech Enhancement | GTCRN (ONNX) |
| Cough Detection | YAMNet (ONNX) |
| Audio Preprocessing | biquad + dagc |
| SOAP Generation | OpenAI-compatible LLM router |
| EMR Integration | Medplum FHIR (OAuth 2.0) |

## Testing

```bash
cd tauri-app

# Frontend tests (437 tests)
pnpm test:run

# Rust tests (670 tests, 29 ignored)
cd src-tauri
ORT_DYLIB_PATH=$(../scripts/setup-ort.sh) cargo test

# E2E integration tests (requires STT Router + LLM Router running)
cargo test e2e_ -- --ignored --nocapture
```

## File Locations

| File | Location |
|------|----------|
| All models | `~/.transcriptionapp/models/` |
| Settings | `~/.transcriptionapp/config.json` |
| Speaker profiles | `~/.transcriptionapp/speaker_profiles.json` |
| Medplum auth | `~/.transcriptionapp/medplum_auth.json` |
| Session archive | `~/.transcriptionapp/archive/YYYY/MM/DD/session_id/` |
| Activity logs | `~/.transcriptionapp/logs/activity.log.*` |
| Debug storage | `~/.transcriptionapp/debug/` (dev builds only) |

## License

MIT
