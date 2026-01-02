# Transcription App

A desktop transcription app that records from the microphone, transcribes offline in near-real-time using Whisper, and copies the final transcript to clipboard.

## Status

**Phase 1 POC - In Development**

- [x] M0: Headless CLI with full audio pipeline
- [x] M1: Tauri skeleton with state machine
- [ ] M2: Integration of CLI pipeline with Tauri UI
- [ ] M3: Polish (permissions, copy, error handling)

## Features

- **Offline Transcription** - Core transcription runs entirely on-device using Whisper
- **Real-time Transcription** - Updates within 1-4 seconds of each utterance
- **VAD-Gated** - Voice Activity Detection prevents hallucinations during silence
- **Speaker Diarization** - Identifies different speakers in conversation
- **SOAP Note Generation** - Optional LLM integration via local Ollama server
- **EMR Integration** - Optional sync to Medplum FHIR server
- **Cross-Platform Ready** - macOS first, Windows architecturally supported

> **Note on Network Features**: The core transcription pipeline is fully offline. Optional features (SOAP notes via Ollama, EMR sync via Medplum) require network access but are disabled by default. The app performs connection checks on startup but functions without them.

## Quick Start

### Prerequisites

- Rust (1.70+)
- Node.js (20+)
- pnpm

### Download Model

```bash
mkdir -p ~/.transcriptionapp/models
curl -L -o ~/.transcriptionapp/models/ggml-small.bin \
  "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
```

### Run CLI

```bash
cd transcribe-cli
cargo run -- --model ~/.transcriptionapp/models/ggml-small.bin
```

### Run Desktop App

```bash
cd tauri-app
pnpm install

# Build debug app (required for OAuth deep links)
pnpm tauri build --debug

# Run with ONNX Runtime (required for transcription, diarization, enhancement)
ORT_DYLIB_PATH=$(./scripts/setup-ort.sh) \
  "src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"
```

> **Note**: Use the debug build instead of `pnpm tauri dev` for proper deep link and single-instance handling.

## Documentation

- [Specification](docs/SPEC.md) - Complete technical spec
- [Development Guide](docs/DEVELOPMENT.md) - Handoff notes for developers

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Tauri App                             │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐      IPC       ┌─────────────────────┐ │
│  │   React/TS UI   │◄──────────────►│    Rust Backend     │ │
│  │  - Transcript   │    Events      │  - Session State    │ │
│  │  - Controls     │                │  - Audio Pipeline   │ │
│  │  - Settings     │                │  - Whisper          │ │
│  └─────────────────┘                └─────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
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
| Biomarkers | YAMNet (cough), pitch/CPP analysis |
| SOAP Generation | Ollama (local LLM) |
| EMR Integration | Medplum FHIR (OAuth 2.0) |

## License

MIT
