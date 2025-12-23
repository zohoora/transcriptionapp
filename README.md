# Transcription App

A desktop transcription app that records from the microphone, transcribes offline in near-real-time using Whisper, and copies the final transcript to clipboard.

## Status

**Phase 1 POC - In Development**

- [x] M0: Headless CLI with full audio pipeline
- [x] M1: Tauri skeleton with state machine
- [ ] M2: Integration of CLI pipeline with Tauri UI
- [ ] M3: Polish (permissions, copy, error handling)

## Features

- **Fully Offline** - No network calls, no analytics, no telemetry
- **Real-time Transcription** - Updates within 1-4 seconds of each utterance
- **VAD-Gated** - Voice Activity Detection prevents hallucinations during silence
- **Cross-Platform Ready** - macOS first, Windows architecturally supported

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
pnpm tauri dev
```

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
| Transcription | whisper-rs |

## License

MIT
