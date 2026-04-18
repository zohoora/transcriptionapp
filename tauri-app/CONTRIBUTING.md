# Contributing to AMI Assist

Guidelines and instructions for contributing to the project.

## Table of Contents

- [Development Setup](#development-setup)
- [Code Style](#code-style)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Architecture Overview](#architecture-overview)
- [Key Modules](#key-modules)

## Development Setup

### Prerequisites

- Node.js 20+
- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- pnpm 10+ (`npm install -g pnpm`)
- ONNX Runtime (for diarization, enhancement, YAMNet)
- Platform-specific dependencies:
  - **macOS**: Xcode Command Line Tools
  - **Ubuntu**: `sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev libasound2-dev`

### Getting Started

```bash
# Clone the repository
git clone https://github.com/zohoora/transcriptionapp.git
cd transcriptionapp/tauri-app

# Install frontend dependencies
pnpm install

# Build debug app (RECOMMENDED over tauri dev)
pnpm tauri build --debug

# Bundle ONNX Runtime
./scripts/bundle-ort.sh "src-tauri/target/debug/bundle/macos/AMI Assist.app"

# Launch
open "src-tauri/target/debug/bundle/macos/AMI Assist.app"
```

**Why not `tauri dev`?**
- Deep link routing (`fabricscribe://oauth/callback`) breaks in dev mode
- `tauri-plugin-single-instance` doesn't work correctly
- OAuth callbacks open new app instances instead of routing to existing one

### IDE Setup

**VS Code Extensions (recommended):**
- rust-analyzer
- ESLint
- Prettier
- Tauri

**JetBrains (RustRover/WebStorm):**
- Rust plugin
- ESLint integration

## Code Style

### TypeScript/React

- Use functional components with hooks
- Prefer named exports
- Use TypeScript strict mode
- Follow ESLint configuration

```typescript
// Good
export function MyComponent({ value }: { value: string }) {
  const [state, setState] = useState(value);
  return <div>{state}</div>;
}

// Avoid
export default class MyComponent extends React.Component { ... }
```

### Rust

- Follow standard Rust conventions
- Use `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Add documentation comments to public items

```rust
/// Processes audio samples through the VAD pipeline.
///
/// # Arguments
///
/// * `samples` - Audio samples at 16kHz
///
/// # Returns
///
/// Detected utterances, if any.
pub fn process(&mut self, samples: &[f32]) -> Vec<Utterance> {
    // ...
}
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add SOAP note generation via LLM router
fix: prevent audio buffer overflow during long recordings
docs: update README with testing instructions
test: add property-based tests for VAD config
refactor: extract audio resampling into separate module
```

## Testing

The full test architecture is documented in **[../docs/TESTING.md](../docs/TESTING.md)** — read it first for context on the 7 test layers, replay tools, labeled corpus, and benchmark fixtures.

Current coverage: 594 frontend tests, 1,125 Rust lib tests, 66 profile-service tests, 192 replay bundles, 68 labeled bundles. All PRs must pass `cargo test --lib`, `pnpm test:run`, and `./scripts/preflight.sh --full`.

### Running Tests

```bash
# Frontend (Vitest, 32 files)
pnpm test:run

# Rust backend (~1,125 lib tests)
cd src-tauri
cargo test --lib

# Profile service (66 tests)
cd ../../profile-service
cargo test

# Full preflight (7 layers, ~30s)
cd ../tauri-app
./scripts/preflight.sh --full

# Frontend coverage
pnpm test:coverage
```

### Test Categories

| Type | Location | Command |
|------|----------|---------|
| Unit tests (TS) | `src/*.test.tsx` | `pnpm test:run` |
| Unit tests (Rust) | `src/*.rs` (mod tests) | `cargo test --lib` |
| Snapshot tests | `src/*.snapshot.test.tsx` | `pnpm test:run` |
| Accessibility | `src/*.a11y.test.tsx` | `pnpm test:run` |
| Contract tests | `src/contracts.test.ts` | `pnpm test:run` |
| Property-based | Rust modules | `cargo test prop_` |
| Stress tests | `src/stress_tests.rs` | `cargo test stress_` |
| Pipeline tests | `src/pipeline_tests.rs` | `cargo test pipeline_` |
| Soak tests (`#[ignore]`) | `src/soak_tests.rs` | `pnpm soak:1h` |
| E2E (`#[ignore]`, requires live STT+LLM Router) | `src/e2e_tests.rs` | `cargo test e2e_ -- --ignored --nocapture` |
| Replay regression CLIs | `tools/*.rs` | `cargo run --bin <name> -- --all` |
| Benchmark fixtures | `tests/fixtures/benchmarks/*.json` | `cargo run --bin benchmark_runner -- <task>` |
| Labeled regression | `tests/fixtures/labels/*.json` | `cargo run --bin labeled_regression_cli -- --all` |
| Golden day | `tests/fixtures/labels/2026-04-15_*.json` | `cargo run --bin golden_day_cli` |

> **Removed in v0.10.31** (April 2026): WebdriverIO E2E (`e2e/`), Playwright visual regression (`tests/visual/`), Stryker mutation testing (`stryker.config.mjs`), `cargo +nightly fuzz` infrastructure. The replay regression CLIs (which test real LLM/data flows) supersede them — see `docs/TESTING.md` for rationale.

### Writing Tests

**Frontend tests:**
```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

describe('MyComponent', () => {
  it('renders correctly', () => {
    render(<MyComponent value="test" />);
    expect(screen.getByText('test')).toBeInTheDocument();
  });
});
```

**Rust tests:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let result = my_function(42);
        assert_eq!(result, 84);
    }
}
```

**Property-based tests:**
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_property(value in 0..1000u32) {
        let result = process(value);
        prop_assert!(result >= value);
    }
}
```

## Pull Request Process

1. **Fork & Branch**: Create a feature branch from `main`
   ```bash
   git checkout -b feat/my-feature
   ```

2. **Develop**: Make your changes with tests

3. **Verify**: Run all checks locally
   ```bash
   pnpm typecheck
   pnpm lint
   pnpm test:run
   cd src-tauri && cargo clippy && cargo test
   ```

4. **Commit**: Use conventional commit messages

5. **Push & PR**: Open a pull request against `main`

6. **Review**: Address feedback from reviewers

7. **Merge**: Squash merge after approval

### PR Checklist

- [ ] Tests pass locally
- [ ] New code has tests
- [ ] Documentation updated (if applicable)
- [ ] No new warnings from clippy/eslint
- [ ] Commit messages follow conventions
- [ ] PR description explains the change

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         React Frontend                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   App.tsx   │  │  Components │  │     AuthProvider        │  │
│  │  (sidebar)  │  │   (modes)   │  │   (Medplum OAuth)       │  │
│  └──────┬──────┘  └──────┬──────┘  └───────────┬─────────────┘  │
│         │                │                     │                 │
│         └────────────────┼─────────────────────┘                 │
│                          │                                       │
│                    IPC (invoke/listen)                           │
└──────────────────────────┼───────────────────────────────────────┘
                           │
┌──────────────────────────┼───────────────────────────────────────┐
│                      Rust Backend                                 │
│                          │                                        │
│  ┌───────────────────────┼───────────────────────┐               │
│  │              Commands (IPC handlers)           │               │
│  └───────────────────────┬───────────────────────┘               │
│                          │                                        │
│  ┌──────────┐  ┌─────────┴─────────┐  ┌──────────┐  ┌──────────┐│
│  │  Audio   │  │  Session Manager  │  │  Config  │  │  Models  ││
│  │ Capture  │  │   (State Machine) │  │          │  │ Download ││
│  └────┬─────┘  └─────────┬─────────┘  └──────────┘  └──────────┘│
│       │                  │                                        │
│       ▼                  ▼                                        │
│  ┌─────────┐      ┌─────────────────────────────────────────┐    │
│  │  Ring   │─────▶│           Processing Pipeline            │    │
│  │ Buffer  │      │  ┌─────┐  ┌─────────┐  ┌─────────────┐  │    │
│  └─────────┘      │  │ VAD │─▶│ Whisper │─▶│ Diarization │  │    │
│                   │  └─────┘  └─────────┘  └─────────────┘  │    │
│                   │       │                                  │    │
│                   │       ▼                                  │    │
│                   │  ┌─────────────┐                        │    │
│                   │  │ Enhancement │                        │    │
│                   │  │  (GTCRN)    │                        │    │
│                   │  └─────────────┘                        │    │
│                   └─────────────────────────────────────────┘    │
│                          │                                        │
│       ┌──────────────────┼──────────────────┐                    │
│       ▼                  ▼                  ▼                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ Biomarkers  │  │ Transcripts │  │     EMR Integration     │  │
│  │  (sidecar)  │  │   Results   │  │  ┌─────────┐ ┌───────┐  │  │
│  │ ┌─────────┐ │  └─────────────┘  │  │ Medplum │ │ LLM   │  │  │
│  │ │ Vitality│ │                   │  │  (FHIR) │ │Router │  │  │
│  │ │Stability│ │                   │  └─────────┘ └───────┘  │  │
│  │ │ YAMNet  │ │                   └─────────────────────────┘  │
│  │ └─────────┘ │                                                 │
│  └─────────────┘                                                 │
└──────────────────────────────────────────────────────────────────┘
```

## Key Modules

### Backend (Rust)

| Module | Purpose |
|--------|---------|
| `commands/` | Tauri IPC command handlers (~21 submodules incl. audio_upload, billing, calibration) |
| `session` | Recording state machine (Idle→Preparing→Recording→Stopping→Completed) |
| `pipeline` | Audio processing coordination (VAD, STT, diarization, enhancement) |
| `audio` | Device enumeration, audio capture, resampling (rubato) |
| `vad` | Voice Activity Detection (Silero VAD) |
| `transcription` | Segment and utterance data types |
| `config` | Settings persistence (JSON) + `replay_snapshot()` |
| `models` | Model download management |
| `checklist` | Pre-flight verification system |
| `diarization/` | Speaker embedding extraction (ONNX) and clustering |
| `enhancement/` | Speech denoising (GTCRN ONNX model) |
| `continuous_mode` | Continuous charting mode (all-day encounter detection) |
| `continuous_mode_events` | Typed event emission for continuous mode |
| `local_archive` | Local session storage and history |
| `speaker_profiles` | Speaker voice enrollment storage |
| `whisper_server` | STT Router client (WebSocket streaming + batch fallback) |
| `listening` | Auto-session detection (VAD + greeting check) |
| `screenshot` | Screen capture (in-memory JPEG for vision) |
| `mcp/` | MCP server on port 7101 |
| `biomarkers/` | Vocal biomarker analysis (vitality, stability, cough) |
| `llm_client` | OpenAI-compatible LLM client for SOAP generation |
| `ollama` | Re-exports from llm_client.rs (backward compat) |
| `medplum` | Medplum FHIR client (OAuth, encounters, documents) |
| `encounter_detection` | Encounter detection prompts/parsing + clinical content check + retrospective multi-patient check |
| `encounter_merge` | Encounter merge prompts/parsing (M1 name-aware strategy) |
| `encounter_pipeline` | Shared helpers (SOAP, merge check, clinical check, billing extraction) |
| `screenshot_task` | Screenshot capture task for continuous mode |
| `patient_name_tracker` | Vision-based patient name + DOB extraction (JSON format) |
| `presence_sensor/` | Multi-sensor presence suite (mmWave + thermal + CO2, SensorSource trait, DebounceFsm, fusion engine) |
| `gemini_client` | Google Gemini API client (AI image generation) |
| `shadow_log` | Shadow mode CSV logging (dual detection comparison) |
| `shadow_observer` | Shadow mode observer task (sensor-side dual detection) |
| `activity_log` | Structured PHI-safe activity logging |
| `pipeline_log` | Pipeline replay JSONL logger |
| `segment_log` | Per-segment JSONL timeline logger (continuous mode) |
| `replay_bundle` | Self-contained encounter replay test case builder (schema v3, includes MultiPatientSplitDecision) |
| `day_log` | Day-level orchestration JSONL logger |
| `performance_summary` | Writes `performance_summary.json` per day at continuous-mode stop. Per-step latency percentiles + scheduling/network split + peak concurrency + failure counts |
| `transcript_buffer` | Timestamped segment buffer (continuous mode) |
| `audio_processing` | Shared ffmpeg + WAV helpers used by manual audio upload + mobile CLI |
| `billing/` | FHO+ billing engine (235 OHIP codes, 562 diagnostic codes, two-stage extraction) |
| `server_sync` | `ServerSyncContext` — fire-and-forget session upload + 30s delayed re-sync |
| `server_config` | Server-configurable prompts/billing/thresholds (3-tier fallback: server → cache → defaults) |
| `room_config` | Room config (room name, profile server URL, fallback URLs, room ID) |
| `physician_cache` | Local cache fallback for physician list + settings |
| `profile_client` | HTTP client for profile service (physicians, sessions, speakers, rooms, config) |
| `audio_upload_queue` | Background audio upload queue for server sync |
| `co2_calibration` | CO2 sensor baseline calibration tool |
| `tools/*.rs` | 12 replay/regression CLIs (detection_replay, merge_replay, clinical_replay, multi_patient_replay, multi_patient_split_replay, benchmark_runner, labeled_regression, golden_day, bootstrap_labels, replay_bundle_backfill, encounter_experiment, vision_experiment) |
| `bin/process_mobile.rs` | Mobile audio processing CLI (polls profile service, runs STT→detect→SOAP) |
| `benches/audio_benchmarks.rs` | Criterion benchmarks for audio processing |

### Frontend (React)

See `tauri-app/CLAUDE.md` "Frontend Structure" section for the canonical full list. Highlights:

| Component | Purpose |
|-----------|---------|
| `App.tsx` | Main sidebar layout, mode routing, modal coordination |
| `modes/ReadyMode` | Pre-recording state (device selection, start button, audio upload link) |
| `modes/RecordingMode` | Active recording (timer, transcript preview, patient handout, DDx) |
| `modes/ReviewMode` | Post-recording (full transcript, SOAP note, EMR sync) |
| `modes/ContinuousMode` | Continuous charting dashboard (live transcript, encounter stats, recent encounters) |
| `AudioUploadModal` | Manual audio file upload (mp3/wav/m4a/...) → ffmpeg → STT → detection → SOAP |
| `SettingsDrawer` | Flat settings panel (continuous mode toggle, mic, SOAP prefs, automation, room, speakers) |
| `Header` | AMI Assist title + version, update banner, history/settings buttons |
| `ActivePhysicianBadge` | Current physician display + switch button |
| `PhysicianSelect` / `RoomSetup` / `RoomSelect` / `AdminPanel` | Multi-user setup + admin |
| `AuthProvider` / `LoginScreen` | Medplum OAuth |
| `PatientSearch` / `EncounterBar` / `SpeakerEnrollment` / `ClinicalChat` | EMR + chat |
| `ImageSuggestions` / `ImageViewerWindow` / `ImageHistoryWindow` | AI image generation (Gemini) or MIIS images |
| `PatientHandoutEditor` | Standalone window for patient handout (Save/Print/Copy/Close) |
| `HistoryWindow` / `HistoryView` / `Calendar` / `AudioPlayer` | Session archive browsing |
| `SplitWindow` | Standalone window for splitting sessions (LLM-suggested split point) |
| `cleanup/` | Session cleanup dialogs (delete, merge, rename, edit) |
| `billing/BillingTab` + `DailySummaryView` + `MonthlySummaryView` + `CapProgressBar` | FHO+ billing UI |
| `CalibrationWindow` | CO2 sensor calibration (standalone window) |
| `FeedbackPanel` | Session feedback/rating |
| `PatientPulse` / `PatientVoiceMonitor` / `BiomarkersSection` / `ConversationDynamicsSection` / `AudioQualitySection` | Voice metric displays |
| `SyncStatusBar` | EMR sync indicator |
| `ErrorBoundary` | React error boundary with fallback UI |

### Shared Types

See `src/types/index.ts` for TypeScript types that mirror Rust backend types:
- `SessionState`, `SessionStatus` - Recording state
- `TranscriptUpdate` - Real-time transcript data
- `BiomarkerUpdate`, `AudioQualitySnapshot` - Metrics
- `SoapNote`, `MultiPatientSoapResult`, `SoapOptions` - SOAP generation
- `LLMStatus` (alias: `OllamaStatus`) - LLM integration
- `AuthState`, `Encounter`, `Patient`, `MedplumSyncState` - Medplum types
- `SpeakerProfileInfo`, `SpeakerRole` - Speaker enrollment
- `LocalArchiveSummary`, `LocalArchiveMetadata`, `LocalArchiveDetails` - Session history
- `ContinuousModeStats`, `ContinuousModeEvent` - Continuous charting mode
- `ListeningEventPayload` - Auto-session detection
- `BillingRecord`, `BillingCode`, `TimeEntry`, `BillingContext` - FHO+ billing
- `AudioUploadProgress`, `AudioUploadResult`, `UploadedSession` - Manual audio upload
- `PatientHandout` - Patient handout editor

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

4. **Add Tauri Command** (`commands/*.rs`):
   - Add command function in the appropriate `commands/` submodule
   - Register in `lib.rs` invoke_handler

5. **Add to Pipeline** (`pipeline.rs`):
   - Add feature-gated provider initialization
   - Integrate into processing loop
   - Add to drop order at end

6. **Add Frontend Types** (`types/index.ts`):
   - Add TypeScript interfaces matching Rust types
   - Update relevant components

## Questions?

- Open a [GitHub Discussion](https://github.com/zohoora/transcriptionapp/discussions)
- Check existing [Issues](https://github.com/zohoora/transcriptionapp/issues)
- See [CLAUDE.md](./CLAUDE.md) for AI coder context

Thank you for contributing!
