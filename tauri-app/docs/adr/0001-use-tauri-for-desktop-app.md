# ADR-0001: Use Tauri for Desktop Application

## Status

Accepted

## Context

We need to build a desktop application for real-time speech transcription. The application requires:

- Native audio capture capabilities
- Low-latency processing
- Cross-platform support (macOS, Windows, Linux)
- Small binary size
- Modern web-based UI

Options considered:
1. **Electron** - Mature, large ecosystem, but large binary size (~150MB+)
2. **Tauri** - Rust backend, smaller binaries (~10MB), native performance
3. **Native apps** - Best performance but requires maintaining separate codebases

## Decision

We chose **Tauri 2.x** as our application framework.

Tauri provides:
- Rust backend for performance-critical audio processing
- React/TypeScript frontend for rapid UI development
- Native system integration via plugins
- Small binary size using system webview
- Strong security model with capability-based permissions

## Consequences

### Positive

- Small application size (~15MB vs 150MB+ for Electron)
- Native Rust performance for audio processing and VAD
- Type-safe IPC between frontend and backend
- Memory-safe backend code
- Active community and Anthropic backing

### Negative

- Smaller ecosystem compared to Electron
- Team needs Rust expertise
- WebView behavior varies slightly across platforms
- Less mature than Electron for edge cases

## References

- [Tauri Documentation](https://tauri.app/)
- [Tauri vs Electron comparison](https://tauri.app/about/intro)
