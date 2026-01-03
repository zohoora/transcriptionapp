# Transcription App - Development Guide

> **Note**: This document has been updated from the original handoff document. For the most comprehensive and up-to-date development context, see [tauri-app/CLAUDE.md](../tauri-app/CLAUDE.md).

## Project Status

**Date:** January 2025
**Status:** Production-Ready - All milestones complete

### Completed Work

1. **M0: Headless CLI** - COMPLETE
   - Full audio capture pipeline with cpal
   - Ring buffer for lock-free audio streaming
   - Resampling from device rate to 16kHz
   - VAD-gated pipeline with Silero VAD
   - Whisper transcription integration

2. **M1: Tauri Skeleton** - COMPLETE
   - React/TypeScript frontend with Vite
   - Tauri 2.x Rust backend
   - Session state machine
   - IPC commands and event emission

3. **M2: Integration** - COMPLETE
   - Audio pipeline integrated with Tauri
   - Real-time transcript updates
   - Device selection in UI
   - Speaker diarization
   - Speech enhancement (GTCRN)
   - Emotion detection (wav2small)

4. **M3: Polish** - COMPLETE
   - macOS microphone permission handling
   - Copy to clipboard functionality
   - Error handling and user feedback
   - Biomarker analysis (vitality, stability, cough detection)
   - Audio quality monitoring

5. **Additional Features** - COMPLETE
   - SOAP note generation via Ollama LLM
   - Audio events in SOAP context (Jan 2025)
   - Medplum EMR integration (OAuth 2.0, FHIR)
   - Encounter history with calendar view
   - Audio preprocessing (DC removal, high-pass, AGC)
   - Conversation dynamics analysis

---

## Project Structure

```
transcriptionapp/
├── docs/
│   ├── SPEC.md              # Original POC specification (historical)
│   ├── DEVELOPMENT.md       # This file
│   └── REVIEW.md            # Code review notes
├── transcribe-cli/          # M0: Headless CLI (reference implementation)
└── tauri-app/               # Main application
    ├── src/                 # React frontend
    ├── src-tauri/           # Rust backend
    ├── docs/adr/            # Architecture Decision Records
    ├── CLAUDE.md            # AI coder context (COMPREHENSIVE)
    ├── CONTRIBUTING.md      # Development guidelines
    └── README.md            # App documentation
```

---

## How to Build and Run

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Node.js 20+
- pnpm 10+ (`npm install -g pnpm`)
- ONNX Runtime (for diarization, enhancement, emotion, YAMNet)

### Desktop App

```bash
cd tauri-app
pnpm install

# Set up ONNX Runtime
./scripts/setup-ort.sh

# Build debug app (RECOMMENDED - required for OAuth deep links)
pnpm tauri build --debug

# Run with ONNX Runtime
ORT_DYLIB_PATH=$(./scripts/setup-ort.sh) \
  "src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"
```

**Why not `tauri dev`?**
- Deep link routing (`fabricscribe://oauth/callback`) breaks in dev mode
- `tauri-plugin-single-instance` doesn't work correctly
- OAuth callbacks open new app instances instead of routing to existing one

---

## Testing

### Frontend Tests (335 tests)

```bash
cd tauri-app
pnpm test:run          # Run once
pnpm test              # Watch mode
pnpm test:coverage     # With coverage
```

### Rust Tests (281 tests)

```bash
cd tauri-app/src-tauri
ORT_DYLIB_PATH=$(../scripts/setup-ort.sh) cargo test
```

---

## Key Documentation

| Document | Purpose |
|----------|---------|
| [tauri-app/CLAUDE.md](../tauri-app/CLAUDE.md) | **Primary reference** - Comprehensive AI coder context |
| [tauri-app/CONTRIBUTING.md](../tauri-app/CONTRIBUTING.md) | Development guidelines, code style, PR process |
| [tauri-app/docs/adr/](../tauri-app/docs/adr/) | Architecture Decision Records |
| [docs/SPEC.md](./SPEC.md) | Original POC specification (historical) |

---

## Architecture Overview

The app follows a clear separation:

- **Frontend (React)**: UI components organized by mode (Ready, Recording, Review)
- **Backend (Rust)**: Session state machine, audio pipeline, integrations
- **IPC**: Commands (frontend → backend) and Events (backend → frontend)

For detailed architecture, see [tauri-app/CLAUDE.md](../tauri-app/CLAUDE.md).

---

## Recent Changes (January 2025)

### Audio Events in SOAP Generation
- YAMNet-detected audio events (coughs, laughs, sneezes) now passed to LLM
- Confidence scores converted to percentages using sigmoid mapping
- Events formatted with timestamps in SOAP prompt
- Cough display removed from UI (used for LLM context only)

### Multi-Window Support
- History window independent from main app
- Closing history window no longer closes entire app

See [tauri-app/CLAUDE.md](../tauri-app/CLAUDE.md) for complete change history.

---

## Model Information

Models are stored at `~/.transcriptionapp/models/`:

| Model | Size | Purpose |
|-------|------|---------|
| `ggml-small.bin` | ~465MB | Whisper transcription (recommended) |
| `speaker_embedding.onnx` | ~26MB | Speaker diarization |
| `gtcrn_simple.onnx` | ~523KB | Speech enhancement |
| `wav2small.onnx` | ~120KB | Emotion detection |
| `yamnet.onnx` | ~3MB | Audio event detection |

---

## Contact / Resources

- **Primary docs**: [tauri-app/CLAUDE.md](../tauri-app/CLAUDE.md)
- **Whisper.cpp**: https://github.com/ggerganov/whisper.cpp
- **Tauri 2.x docs**: https://v2.tauri.app
- **Medplum docs**: https://www.medplum.com/docs
- **Ollama docs**: https://ollama.ai/docs
