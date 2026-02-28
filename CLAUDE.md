# Transcription App

Clinical ambient scribe for physicians — real-time speech-to-text transcription desktop app.

## Repository Structure

```
transcriptionapp/
├── tauri-app/              # Main application (all source code lives here)
│   ├── src/                # React + TypeScript frontend
│   ├── src-tauri/          # Rust + Tauri v2 backend
│   ├── CLAUDE.md           # Detailed codebase context (architecture, commands, patterns)
│   ├── CONTRIBUTING.md     # Development workflow
│   └── README.md           # App-level docs
├── docs/                   # Historical review documents
│   └── *.md                # Code review docs (ADRs live in tauri-app/docs/adr/)
├── scripts/                # Build and preflight scripts
└── CODE_REVIEW_FINDINGS.md # Review findings tracker
```

## Quick Reference

```bash
# Build (NOT tauri dev — OAuth deep links break in dev mode)
cd tauri-app && pnpm tauri build --debug

# Type checks
cd tauri-app && npx tsc --noEmit          # Frontend
cd tauri-app/src-tauri && cargo check     # Backend

# Tests
cd tauri-app && pnpm test:run             # Frontend (Vitest, 414 tests)
cd tauri-app/src-tauri && cargo test      # Backend (561 tests, 32 E2E ignored)

# E2E (requires live STT + LLM Router)
cd tauri-app/src-tauri && cargo test e2e_ -- --ignored --nocapture

# Daily preflight
./scripts/preflight.sh                    # Quick (~10s)
./scripts/preflight.sh --full             # Full (~30s)
```

## Detailed Context

See **[tauri-app/CLAUDE.md](tauri-app/CLAUDE.md)** for full architecture, IPC commands, code patterns, gotchas, and troubleshooting.
