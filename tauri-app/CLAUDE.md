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
├── emotion/         # Emotion detection (wav2small)
│   ├── mod.rs       # Module exports
│   └── provider.rs  # ONNX-based ADV detection
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
| `download_whisper_model` | Download Whisper model |
| `download_speaker_model` | Download speaker diarization model |
| `download_enhancement_model` | Download GTCRN enhancement model |
| `download_emotion_model` | Download wav2small emotion model |
| `download_yamnet_model` | Download YAMNet cough detection model |
| `ensure_models` | Download all required models |

## Key Events (Backend → Frontend)

| Event | Payload |
|-------|---------|
| `session_status` | `{ state, provider, elapsed_ms, is_processing_behind, error_message? }` |
| `transcript_update` | `{ finalized_text, draft_text, segment_count }` |
| `biomarker_update` | `{ cough_count, cough_rate_per_min, turn_count, vitality_session_mean, stability_session_mean, ... }` |
| `audio_quality` | `{ timestamp_ms, peak_db, rms_db, snr_db, clipped_ratio, dropout_count, ... }` |

## Session States

```
Idle → Preparing → Recording → Stopping → Completed
  ↑                    ↓           ↓          ↓
  └────── Error ←──────┴───────────┴──────────┘
  ↑                                            │
  └─────────────── Reset ←─────────────────────┘
```

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

### Emotion Detection (wav2small)
- Dimensional emotion: Arousal, Dominance, Valence (ADV)
- ~120KB model, ~9ms latency, 72K parameters
- Labels: excited/happy, angry/frustrated, calm/content, sad/tired
- Emotion stored in transcript segments

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

**Architecture**
- Sidecar thread runs in parallel with transcription
- Clone Point 1: After resample → YAMNet (all audio)
- Clone Point 2: At utterance → Vitality/Stability (original unenhanced audio)
- Non-blocking: biomarker processing doesn't affect transcription latency
- Real-time UI display via `biomarker_update` event (throttled to 2Hz)

**UI Display**
- Collapsible biomarkers section between Record and Transcript sections
- Vitality/Stability shown as color-coded progress bars (green/yellow/red)
- Cough count badge with rate per minute
- Session metrics (turns, balance) when diarization enabled
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

### Launch Sequence Checklist
- Pre-flight verification system in `checklist.rs`
- Checks: audio devices, models, configuration
- Status types: Pass, Fail, Warning, Skipped
- Extensible for future features (see module docs)

### Test Updates
- All frontend tests updated for new sidebar UI (119+ tests)
- Audio quality tests: 16 Rust unit tests, 12 frontend tests
- Fixed clustering.rs bug where max_speakers wasn't enforced
- All Rust tests passing (175+ tests)

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
}
```

## File Locations

- **All models**: `~/.transcriptionapp/models/`
  - Whisper: `ggml-{tiny,base,small,medium,large}.bin`
  - Speaker: `speaker_embedding.onnx` (~26MB)
  - Enhancement: `gtcrn_simple.onnx` (~523KB)
  - Emotion: `wav2small.onnx` (~120KB)
  - YAMNet: `yamnet.onnx` (~3MB) - for cough detection
- **Settings**: `~/.transcriptionapp/config.json`
- **Logs**: Console (tracing crate)

## Common Issues

1. **"Model not found"**: Ensure Whisper model file exists at configured path
2. **ONNX tests failing**: Set `ORT_DYLIB_PATH` to ONNX Runtime library
3. **Audio device errors**: Check microphone permissions (macOS: System Settings → Privacy)

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

## ADRs

See `docs/adr/` for Architecture Decision Records:
- 0001: Use Tauri for desktop app
- 0002: Whisper for transcription
- 0003: VAD-gated processing
- 0004: Ring buffer audio pipeline
- 0005: Session state machine
- 0006: Speaker diarization (online clustering)
