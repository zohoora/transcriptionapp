# Transcription App - Development Handoff Document

## Project Status Summary

**Date:** December 2024
**Status:** M0-M1 Complete, M2-M3 Pending

### Completed Work

1. **M0: Headless CLI** - COMPLETE
   - Full audio capture pipeline with cpal
   - Ring buffer for lock-free audio streaming
   - Resampling from device rate to 16kHz
   - VAD-gated pipeline with Silero VAD
   - Whisper transcription integration
   - All unit tests passing

2. **M1: Tauri Skeleton** - COMPLETE
   - React/TypeScript frontend with Vite
   - Tauri 2.x Rust backend
   - Session state machine
   - IPC commands for start/stop/reset
   - Event emission for status updates
   - Basic UI layout matching spec

3. **Whisper Model** - DOWNLOADED
   - Model location: `~/.transcriptionapp/models/ggml-small.bin`
   - Size: 465MB

### Remaining Work

1. **M2: Integration** - NOT STARTED
   - Connect CLI audio pipeline to Tauri backend
   - Wire up real-time transcript updates
   - Implement device selection in UI

2. **M3: Polish** - NOT STARTED
   - macOS microphone permission handling
   - Copy to clipboard functionality
   - Error handling and user feedback
   - Performance optimization

---

## Project Structure

```
transcriptionapp/
├── docs/
│   ├── SPEC.md              # Master specification v3.3
│   └── DEVELOPMENT.md       # This file
├── transcribe-cli/          # M0: Headless CLI
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs          # CLI entry point
│       ├── config.rs        # Configuration
│       ├── audio/
│       │   ├── mod.rs
│       │   ├── capture.rs   # Audio capture with cpal
│       │   ├── resampler.rs # Rubato resampling
│       │   └── processor.rs # Processing thread
│       ├── vad/
│       │   ├── mod.rs
│       │   └── pipeline.rs  # VAD-gated pipeline
│       └── transcription/
│           ├── mod.rs
│           ├── segment.rs   # Segment data model
│           └── whisper_provider.rs # Whisper integration
└── tauri-app/               # M1-M3: Tauri Desktop App
    ├── package.json
    ├── vite.config.ts
    ├── tsconfig.json
    ├── index.html
    ├── src/                 # React frontend
    │   ├── main.tsx
    │   ├── App.tsx          # Main app component
    │   └── styles.css       # UI styles
    └── src-tauri/           # Rust backend
        ├── Cargo.toml
        ├── tauri.conf.json
        ├── entitlements.plist
        └── src/
            ├── main.rs
            ├── lib.rs
            ├── commands.rs   # Tauri IPC commands
            ├── config.rs     # Settings management
            ├── session.rs    # Session state machine
            ├── audio.rs      # Audio device listing
            ├── transcription.rs # Segment models
            └── vad.rs        # VAD config
```

---

## How to Build and Run

### Prerequisites

- Rust (installed via Homebrew)
- Node.js v20+
- pnpm

### CLI (M0)

```bash
cd transcribe-cli
cargo build --release

# Run with a model
./target/release/transcribe-cli --model ~/.transcriptionapp/models/ggml-small.bin

# List audio devices
./target/release/transcribe-cli --list-devices
```

### Tauri App

```bash
cd tauri-app
pnpm install
pnpm tauri dev    # Development mode
pnpm tauri build  # Production build
```

---

## Key Integration Points for M2

The main work for M2 is connecting the CLI's audio pipeline to the Tauri backend. Here's how:

### 1. Audio Pipeline Integration

The CLI's `transcribe-cli/src/audio/` module contains the complete audio pipeline:

- `capture.rs`: Creates audio streams with cpal
- `resampler.rs`: Converts to 16kHz
- `processor.rs`: Runs the processing loop

To integrate:

1. Move core audio code to a shared library crate (or copy into tauri-app)
2. Start the processing thread when session starts
3. Send transcript updates via Tauri events

### 2. Event Flow

```
[Audio Capture] → [Ring Buffer] → [Processor Thread] → [VAD] → [Whisper] → [Tauri Event]
                                                                              ↓
                                                                         [React UI]
```

### 3. Key Files to Modify

**tauri-app/src-tauri/src/commands.rs:**
```rust
// In start_session(), after transitioning to Recording:
// 1. Create ring buffer
// 2. Start audio capture
// 3. Spawn processor thread
// 4. Set up channel for receiving transcripts
// 5. Forward transcripts to UI via app.emit()
```

**tauri-app/src-tauri/src/session.rs:**
```rust
// Add fields for:
// - Ring buffer producer/consumer
// - Processor thread handle
// - Transcript receiver channel
```

### 4. Transcript Event Structure

The frontend expects:
```typescript
interface TranscriptUpdate {
  finalized_text: string;
  draft_text: string | null;
  segment_count: number;
}
```

---

## Known Issues / TODOs

### High Priority

1. **M2 Integration**: The `start_session` command currently just transitions states without starting actual audio capture. See `commands.rs:97-100`.

2. **Copy Functionality**: The frontend has a Copy button but the backend `copy_transcript` command is not implemented.

3. **Device Selection**: Device dropdown is populated but selection is not used when starting capture.

### Medium Priority

1. **macOS Permissions**: The entitlements.plist is set up but actual permission request flow needs testing.

2. **Draft Text**: The spec mentions showing draft/partial transcription. This is stubbed but not implemented.

3. **Processing Behind Indicator**: The orange "Processing..." badge logic needs the actual pending count from the audio pipeline.

### Low Priority

1. **Settings Persistence**: Config save/load is implemented but settings UI is not.

2. **Model Selection**: Currently hardcoded to "small" model.

3. **Unused Code Warnings**: Several items are defined but not used yet (intended for M2).

---

## Architecture Notes

### State Machine

The session state machine follows this flow:
```
Idle → Preparing → Recording → Stopping → Completed → Idle
                      ↓              ↓
                    Error ←──────────
```

Transitions are in `session.rs`. The state is serialized and sent to the frontend.

### VAD Gating

The VAD pipeline (from CLI) does NOT drop audio. It:
1. Accumulates audio into speech buffer when speech is detected
2. Includes 300ms pre-roll for soft consonants
3. Flushes to transcription after 500ms silence
4. Enforces 25s max utterance to stay under Whisper's limit

### Ring Buffer

Uses `ringbuf` crate for lock-free SPSC communication between:
- Audio capture callback (producer) - HIGH PRIORITY
- Processing thread (consumer) - Can allocate

The capture callback must be allocation-free.

### Timestamps

Timestamps are sample-based (16kHz), not wall-clock. This prevents drift when processing falls behind.

---

## Testing

### CLI Tests
```bash
cd transcribe-cli
cargo test
```

All 17 tests should pass.

### Tauri Tests
```bash
cd tauri-app/src-tauri
cargo test
```

---

## Model Information

The Whisper model is at: `~/.transcriptionapp/models/ggml-small.bin`

To use a different model:
1. Download from: https://huggingface.co/ggerganov/whisper.cpp/tree/main
2. Place in `~/.transcriptionapp/models/`
3. Update config or pass `--model` flag

Available models:
- `ggml-tiny.bin` (~75MB) - Fastest, lower quality
- `ggml-base.bin` (~145MB)
- `ggml-small.bin` (~465MB) - Recommended
- `ggml-medium.bin` (~1.5GB)
- `ggml-large.bin` (~3GB) - Best quality, slowest

---

## Next Steps for AI Coder

1. **Read the spec** in `docs/SPEC.md` - it has all the details

2. **Understand the CLI** - it's a working reference implementation

3. **Focus on M2** - the main task is:
   - Copy audio pipeline code from CLI to Tauri
   - Wire up the processing thread
   - Emit transcript events to the frontend

4. **Test incrementally** - verify audio capture works before adding VAD, then Whisper

5. **Check macOS permissions** - you may need to run from Terminal first to get mic permission

---

## Contact / Resources

- Spec: `docs/SPEC.md`
- Whisper.cpp: https://github.com/ggerganov/whisper.cpp
- Tauri 2.x docs: https://v2.tauri.app
- cpal: https://docs.rs/cpal
- voice_activity_detector: https://docs.rs/voice_activity_detector
