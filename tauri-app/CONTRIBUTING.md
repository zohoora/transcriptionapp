# Contributing to Transcription App

Thank you for your interest in contributing! This document provides guidelines and instructions for contributing to the project.

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
git clone https://github.com/your-org/transcription-app.git
cd transcription-app/tauri-app

# Install frontend dependencies
pnpm install

# Set up ONNX Runtime
./scripts/setup-ort.sh

# Build debug app (RECOMMENDED over tauri dev)
pnpm tauri build --debug

# Run with ONNX Runtime
ORT_DYLIB_PATH=$(./scripts/setup-ort.sh) \
  "src-tauri/target/debug/bundle/macos/Transcription App.app/Contents/MacOS/transcription-app"
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

We maintain comprehensive test coverage (387 frontend tests, 421 Rust tests). All PRs must pass tests.

### Running Tests

```bash
# Frontend tests (387 tests)
pnpm test:run

# Rust tests with ONNX Runtime (421 tests)
cd src-tauri
ORT_DYLIB_PATH=$(../scripts/setup-ort.sh) cargo test

# All tests with coverage
pnpm test:coverage
cd src-tauri && cargo llvm-cov
```

### Test Categories

| Type | Location | Command |
|------|----------|---------|
| Unit tests (TS) | `src/*.test.tsx` | `pnpm test:run` |
| Unit tests (Rust) | `src/*.rs` (mod tests) | `cargo test` |
| Snapshot tests | `src/*.snapshot.test.tsx` | `pnpm test:run` |
| Accessibility | `src/*.a11y.test.tsx` | `pnpm test:run` |
| Contract tests | `src/contracts.test.ts` | `pnpm test:run` |
| Property-based | Rust modules | `cargo test prop_` |
| Stress tests | `src/stress_tests.rs` | `cargo test stress_` |
| Pipeline tests | `src/pipeline_tests.rs` | `cargo test pipeline_` |
| Visual regression | `tests/visual/` | `pnpm visual:test` |
| E2E tests | `e2e/` | `pnpm e2e` |
| Fuzz tests | `fuzz/` | `cargo +nightly fuzz run` |
| Mutation tests | - | `pnpm mutation:test` / `cargo mutants` |
| Soak tests | `src/soak_tests.rs` | `pnpm soak:test` |

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
| `commands/` | Tauri IPC command handlers (16 submodules) |
| `session` | Recording state machine (Idle→Preparing→Recording→Stopping→Completed) |
| `pipeline` | Audio processing coordination (VAD, Whisper, diarization, enhancement) |
| `audio` | Device enumeration, audio capture, resampling (rubato) |
| `vad` | Voice Activity Detection (Silero VAD) |
| `transcription` | Segment and utterance data types |
| `config` | Settings persistence (JSON) |
| `models` | Model download management |
| `checklist` | Pre-flight verification system |
| `diarization/` | Speaker embedding extraction (ONNX) and clustering |
| `enhancement/` | Speech denoising (GTCRN ONNX model) |
| `continuous_mode` | Continuous charting mode (end-of-day encounter detection) |
| `local_archive` | Local session storage and history |
| `speaker_profiles` | Speaker voice enrollment storage |
| `whisper_server` | STT Router client (WebSocket streaming) |
| `listening` | Auto-session detection (VAD + greeting check) |
| `screenshot` | Screen capture (in-memory JPEG for vision) |
| `mcp/` | MCP server on port 7101 for IT Admin Coordinator |
| `biomarkers/` | Vocal biomarker analysis (vitality, stability, cough) |
| `llm_client` | OpenAI-compatible LLM client for SOAP generation |
| `ollama` | Re-exports from llm_client.rs (backward compat) |
| `medplum` | Medplum FHIR client (OAuth, encounters, documents) |
| `activity_log` | Structured PHI-safe activity logging |

### Frontend (React)

| Component | Purpose |
|-----------|---------|
| `App.tsx` | Main sidebar layout, state management |
| `modes/ReadyMode` | Pre-recording state (device selection, start button) |
| `modes/RecordingMode` | Active recording (timer, transcript preview) |
| `modes/ReviewMode` | Post-recording (full transcript, SOAP note, sync) |
| `modes/ContinuousMode` | Continuous charting dashboard (monitoring, encounters) |
| `AudioQualitySection` | Real-time audio level/SNR display |
| `BiomarkersSection` | Vitality, stability, cough metrics |
| `ConversationDynamicsSection` | Turn-taking, overlap, response latency |
| `PatientPulse` | Glanceable biomarker summary |
| `PatientVoiceMonitor` | Patient-focused voice metric trending |
| `SettingsDrawer` | Configuration panel |
| `Header` | App title, history button, settings button |
| `AuthProvider` | Medplum OAuth context |
| `LoginScreen` | Medplum login UI |
| `PatientSearch` | FHIR patient search |
| `EncounterBar` | Active encounter display |
| `SpeakerEnrollment` | Speaker voice enrollment |
| `ClinicalChat` | Clinical assistant chat panel |
| `ImageSuggestions` | MIIS medical illustration display |
| `SyncStatusBar` | EMR sync status indicator |
| `HistoryWindow` | Separate window for encounter history |
| `HistoryView` | Encounter history list |
| `Calendar` | Date picker for history |
| `AudioPlayer` | Playback of recorded audio |

### Shared Types

See `src/types/index.ts` for TypeScript types that mirror Rust backend types:
- `SessionState`, `SessionStatus` - Recording state
- `TranscriptUpdate` - Real-time transcript data
- `BiomarkerUpdate`, `AudioQualitySnapshot` - Metrics
- `SoapNote`, `MultiPatientSoapResult`, `SoapOptions` - SOAP generation
- `LLMStatus` (alias: `OllamaStatus`) - LLM integration
- `AuthState`, `Encounter`, `Patient` - Medplum types
- `SpeakerProfileInfo`, `SpeakerRole` - Speaker enrollment
- `LocalArchiveSummary`, `LocalArchiveMetadata`, `LocalArchiveDetails` - Session history
- `ContinuousModeStats`, `ContinuousModeEvent` - Continuous charting mode
- `ListeningEventPayload` - Auto-session detection

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

- Open a [GitHub Discussion](https://github.com/your-org/transcription-app/discussions)
- Check existing [Issues](https://github.com/your-org/transcription-app/issues)
- See [CLAUDE.md](./CLAUDE.md) for AI coder context

Thank you for contributing!
