# Contributing to Transcription App

Thank you for your interest in contributing! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Development Setup](#development-setup)
- [Code Style](#code-style)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Architecture Overview](#architecture-overview)

## Development Setup

### Prerequisites

- Node.js 20+
- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- pnpm 10+ (`npm install -g pnpm`)
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

# Run in development mode
pnpm tauri dev
```

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
feat: add support for multiple audio devices
fix: prevent audio buffer overflow during long recordings
docs: update README with testing instructions
test: add property-based tests for VAD config
refactor: extract audio resampling into separate module
```

## Testing

We maintain comprehensive test coverage. All PRs must pass tests.

### Running Tests

```bash
# Frontend tests
pnpm test:run

# Rust tests
cd src-tauri && cargo test

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
| Visual regression | `tests/visual/` | `pnpm visual:test` |
| E2E tests | `e2e/` | `pnpm e2e` |
| Fuzz tests | `fuzz/` | `cargo +nightly fuzz run` |
| Mutation tests | - | `pnpm mutation:test` / `cargo mutants` |

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
┌─────────────────────────────────────────────────────────┐
│                    React Frontend                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │    App.tsx  │  │   Hooks     │  │  Components │     │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘     │
│         │                │                │             │
│         └────────────────┼────────────────┘             │
│                          │                              │
│                    IPC (invoke/listen)                  │
└──────────────────────────┼──────────────────────────────┘
                           │
┌──────────────────────────┼──────────────────────────────┐
│                    Rust Backend                         │
│                          │                              │
│  ┌───────────────────────┼───────────────────────┐     │
│  │              Commands (IPC handlers)           │     │
│  └───────────────────────┬───────────────────────┘     │
│                          │                              │
│  ┌──────────┐  ┌─────────┴─────────┐  ┌──────────┐    │
│  │  Audio   │  │  Session Manager  │  │  Config  │    │
│  │ Capture  │  │   (State Machine) │  │          │    │
│  └────┬─────┘  └─────────┬─────────┘  └──────────┘    │
│       │                  │                             │
│       ▼                  ▼                             │
│  ┌─────────┐      ┌─────────────┐                     │
│  │  Ring   │─────▶│   Pipeline  │                     │
│  │ Buffer  │      │  (VAD+Whisper)                    │
│  └─────────┘      └──────┬──────┘                     │
│                          │                             │
│                          ▼                             │
│                   ┌─────────────┐                      │
│                   │ Transcription│                     │
│                   │   Results    │                     │
│                   └─────────────┘                      │
└─────────────────────────────────────────────────────────┘
```

### Key Modules

| Module | Purpose |
|--------|---------|
| `audio` | Device enumeration, audio capture, resampling |
| `vad` | Voice Activity Detection, utterance detection |
| `session` | Recording state machine, segment management |
| `transcription` | Segment and utterance data types |
| `config` | Settings persistence and management |
| `commands` | Tauri IPC command handlers |
| `pipeline` | Audio processing pipeline coordination |

## Questions?

- Open a [GitHub Discussion](https://github.com/your-org/transcription-app/discussions)
- Check existing [Issues](https://github.com/your-org/transcription-app/issues)

Thank you for contributing!
