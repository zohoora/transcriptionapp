# Transcription App

A real-time speech-to-text transcription desktop application built with Tauri, React, and Rust. Designed as a clinical ambient scribe for physicians, running as a compact 300px sidebar alongside EMR systems.

## Status

**Production-Ready** - All core features implemented and tested.

## Features

### Core Transcription
- **Offline Transcription** - Core transcription runs entirely on-device using Whisper
- **Real-time Updates** - Finalized transcript within 1-4 seconds of each utterance
- **VAD-Gated** - Voice Activity Detection prevents hallucinations during silence
- **Speaker Diarization** - Identifies up to 10 different speakers in conversation
- **Speech Enhancement** - GTCRN denoising for cleaner audio (~2ms latency)
- **Emotion Detection** - Arousal, Dominance, Valence analysis via wav2small

### Clinical Features
- **SOAP Note Generation** - AI-powered clinical notes via OpenAI-compatible LLM router
- **Multi-Patient SOAP** - Supports up to 4 patients per visit with auto-detection
- **Audio Event Context** - Coughs, laughs, sneezes included in SOAP generation
- **Medplum EMR Integration** - OAuth 2.0 + PKCE, FHIR resources
- **Auto-Sync to EMR** - Transcripts and audio automatically synced on session complete
- **Encounter History** - Browse past sessions with calendar view
- **Auto-Session Detection** - Automatically starts recording when greeting detected

### Biomarker Analysis
- **Vitality** - Pitch variability for affect detection (depression/PTSD indicator)
- **Stability** - CPP measurement for vocal control (Parkinson's indicator)
- **Cough Detection** - YAMNet-based audio event classification
- **Conversation Dynamics** - Turn-taking, overlap, response latency, engagement score

### Audio Quality Monitoring
- **Real-time Levels** - Peak, RMS, SNR monitoring
- **Clipping Detection** - Warns when audio is too loud
- **Noise Floor Analysis** - Ambient noise level tracking
- **Actionable Suggestions** - Context-aware feedback

> **Note on Network Features**: The core transcription pipeline can run offline with local Whisper. Optional features (SOAP notes via LLM router, EMR sync via Medplum, remote Whisper) require network access.

## Quick Start

### Prerequisites

- Rust 1.70+
- Node.js 20+
- pnpm 10+
- ONNX Runtime (for diarization, enhancement, emotion, YAMNet)

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

- **[tauri-app/CLAUDE.md](tauri-app/CLAUDE.md)** - Comprehensive AI coder context
- **[tauri-app/CONTRIBUTING.md](tauri-app/CONTRIBUTING.md)** - Development guidelines
- **[tauri-app/README.md](tauri-app/README.md)** - App-specific documentation
- **[tauri-app/docs/adr/](tauri-app/docs/adr/)** - Architecture Decision Records

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Tauri App                                   │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐      IPC       ┌─────────────────────────────────┐ │
│  │   React/TS UI   │◄──────────────►│          Rust Backend           │ │
│  │  - Sidebar      │    Events      │  - Session State Machine        │ │
│  │  - Settings     │                │  - Audio Pipeline               │ │
│  │  - Transcript   │                │  - Whisper + Diarization        │ │
│  │  - SOAP Notes   │                │  - Biomarker Analysis           │ │
│  │  - EMR Sync     │                │  - LLM Router + Medplum         │ │
│  └─────────────────┘                └─────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
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
| Transcription | whisper-rs (GGML) |
| Speaker Diarization | ONNX Runtime + WeSpeaker |
| Speech Enhancement | GTCRN (ONNX) |
| Emotion Detection | wav2small (ONNX) |
| Cough Detection | YAMNet (ONNX) |
| Audio Preprocessing | biquad + dagc |
| SOAP Generation | OpenAI-compatible LLM router |
| EMR Integration | Medplum FHIR (OAuth 2.0) |

## Testing

```bash
cd tauri-app

# Frontend tests (430 tests)
pnpm test:run

# Rust tests (346 tests)
cd src-tauri
ORT_DYLIB_PATH=$(../scripts/setup-ort.sh) cargo test
```

## File Locations

| File | Location |
|------|----------|
| All models | `~/.transcriptionapp/models/` |
| Settings | `~/.transcriptionapp/config.json` |
| Medplum auth | `~/.transcriptionapp/medplum_auth.json` |
| Activity logs | `~/.transcriptionapp/logs/activity.log.*` |

## License

MIT
