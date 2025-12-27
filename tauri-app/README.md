# Transcription App

A real-time speech-to-text transcription application built with Tauri, React, and Rust.

[![CI](https://github.com/your-org/transcription-app/actions/workflows/ci.yml/badge.svg)](https://github.com/your-org/transcription-app/actions/workflows/ci.yml)
[![Frontend Coverage](https://img.shields.io/badge/frontend%20coverage-70%25-brightgreen)](./tests/visual/playwright-report)
[![Rust Coverage](https://img.shields.io/badge/rust%20coverage-60%25-brightgreen)](./src-tauri/target/llvm-cov)

## Features

- Real-time audio transcription using Whisper
- Voice Activity Detection (VAD) for smart audio segmentation
- Speaker diarization with configurable max speakers
- Compact sidebar UI designed for clinical ambient scribe use
- Support for multiple audio input devices
- Copy transcript to clipboard
- Paragraph-formatted output with speaker labels

## Requirements

- Node.js 20+
- Rust 1.70+
- pnpm 10+
- Whisper model file (ggml-small.bin or similar)

## Quick Start

```bash
# Install dependencies
pnpm install

# Run in development mode
pnpm tauri dev

# Build for production
pnpm tauri build
```

## Project Structure

```
tauri-app/
├── src/                    # React frontend
│   ├── App.tsx            # Main sidebar component
│   ├── ErrorBoundary.tsx  # Error handling component
│   ├── styles.css         # Light mode sidebar styles
│   └── test/              # Test mocks and utilities
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── audio.rs       # Audio capture and resampling
│   │   ├── config.rs      # Settings management
│   │   ├── session.rs     # Recording session state
│   │   ├── transcription.rs # Segment/utterance types
│   │   ├── vad.rs         # Voice Activity Detection
│   │   └── diarization/   # Speaker diarization module
│   │       ├── mod.rs     # Embedding extraction
│   │       ├── clustering.rs # Online speaker clustering
│   │       └── config.rs  # Diarization settings
│   ├── benches/           # Performance benchmarks
│   └── fuzz/              # Fuzz testing targets
├── e2e/                   # End-to-end tests (WebDriver)
└── tests/visual/          # Visual regression tests (Playwright)
```

## Testing

### Frontend Tests

```bash
# Run all tests
pnpm test

# Run tests once (CI mode)
pnpm test:run

# Run with coverage
pnpm test:coverage

# Visual regression tests
pnpm visual:test
pnpm visual:update  # Update baselines

# Mutation testing
pnpm mutation:test
```

### Rust Tests

```bash
cd src-tauri

# Run all tests
cargo test

# Run specific test module
cargo test vad::tests

# Run stress tests
cargo test stress_test

# Run benchmarks
cargo bench

# Mutation testing (requires cargo-mutants)
cargo mutants

# Fuzz testing (requires nightly)
cargo +nightly fuzz run fuzz_vad_config
```

### E2E Tests

```bash
# Build the app first
pnpm tauri build

# Install tauri-driver
cargo install tauri-driver

# Run E2E tests
pnpm e2e
```

### Soak Tests (Long-running stability)

```bash
# Quick soak test (1 minute)
pnpm soak:quick

# 1-hour soak test
pnpm soak:1h

# Interactive soak test script
pnpm soak:test

# Run specific soak test directly
cd src-tauri
SOAK_DURATION_SECS=3600 cargo test --release soak_test_extended_vad_pipeline -- --ignored --nocapture
```

Available soak tests:
- `soak_test_extended_vad_pipeline` - VAD pipeline under sustained load
- `soak_test_extended_session_management` - Session lifecycle stress
- `soak_test_extended_resampling` - Audio resampling throughput
- `soak_test_concurrent_operations` - Multi-threaded operations
- `soak_test_memory_stress` - Memory allocation/deallocation

## Test Categories

| Category | Framework | Coverage |
|----------|-----------|----------|
| Unit Tests (Frontend) | Vitest | 119 tests |
| Unit Tests (Rust) | cargo test | 160 tests |
| Snapshot Tests | Vitest | 7 snapshots |
| Accessibility Tests | vitest-axe | 12 tests |
| Contract Tests | Vitest | 24 tests |
| Property-based Tests | proptest | 17 tests |
| Stress Tests | cargo test | 11 tests |
| Pipeline Integration | cargo test | 10 tests |
| Visual Regression | Playwright | 15+ tests |
| E2E Tests | WebDriverIO | 20+ tests |
| Soak Tests | cargo test | 5 tests |

## Architecture

See [docs/adr/](./docs/adr/) for Architecture Decision Records.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

## License

MIT
