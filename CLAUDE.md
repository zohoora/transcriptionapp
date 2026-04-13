# AMI Assist

Ambient Medical Intelligence — clinical ambient scribe for physicians. Real-time speech-to-text transcription desktop app with automated encounter detection, SOAP note generation, and multi-room clinic deployment.

## Repository Structure

```
transcriptionapp/
├── tauri-app/              # Desktop application (Tauri v2 + React + Rust)
│   ├── src/                # React + TypeScript frontend
│   ├── src-tauri/          # Rust backend + CLI binaries
│   │   └── src/bin/        # CLI tools (process_mobile, detection_replay, etc.)
│   ├── CLAUDE.md           # Detailed codebase context (architecture, commands, patterns)
│   ├── CONTRIBUTING.md     # Development workflow
│   └── README.md           # App-level docs
├── profile-service/        # Standalone axum REST API for multi-user management
│   ├── src/                # Physician profiles, rooms, sessions, speaker enrollments, mobile jobs
│   └── CLAUDE.md           # Profile service architecture and patterns
├── ios/                    # iOS mobile app (SwiftUI) — offline recorder + auto-upload
│   ├── AMI Assist/         # Source (Views, Services, Models)
│   └── project.yml         # XcodeGen project spec
├── esp32-presence/         # ESP32 sensor firmware (PlatformIO)
├── room6-xiao-sensor/     # XIAO ESP32-C3 mmWave sensor firmware (Arduino)
├── .github/workflows/      # CI + Release (auto-update via GitHub Releases)
├── docs/                   # Historical review documents, benchmarks, plans
│   └── *.md                # Code review docs (ADRs live in tauri-app/docs/adr/)
├── scripts/                # Data export and sensor logging scripts
└── CODE_REVIEW_FINDINGS.md # Review findings tracker
```

## Clinic Deployment

| Machine | Role | IP (Tailscale) | LAN IP | User | Location |
|---------|------|----------------|--------|------|----------|
| MacBook | Server (profile service, STT Router, LLM Router, Medplum) | 100.119.83.76 | 10.241.15.154 | arash | Server room |
| iMac | Room 2 clinic workstation | 100.74.186.113 | — | room2 (password: 1278) | Room 2 |
| This computer | Room 6 clinic workstation | (local) | — | backoffice | Room 6 |

Workstations have `fallback_server_urls` in `room_config.json` so the app automatically falls back to the LAN IP if Tailscale is down.

### Services on MacBook (100.119.83.76 / 10.241.15.154)

| Service | Port | Purpose |
|---------|------|---------|
| Profile Service | 8090 | Physician profiles, rooms, sessions, speaker sync, mobile job tracking |
| STT Router | 8001 | WebSocket streaming transcription (alias: `medical-streaming`) |
| LLM Router | 8080 | SOAP generation, encounter detection, vision |
| Medplum | 8103 | EMR/FHIR server |
| Processing CLI | — | `process_mobile` daemon: polls profile service, processes mobile recordings via STT+LLM |

## Quick Reference

```bash
# Build (NOT tauri dev — OAuth deep links break in dev mode)
cd tauri-app && pnpm tauri build --debug

# Bundle ONNX Runtime (required after build)
./scripts/bundle-ort.sh "src-tauri/target/debug/bundle/macos/AMI Assist.app"

# Launch
open "src-tauri/target/debug/bundle/macos/AMI Assist.app"

# Type checks
cd tauri-app && npx tsc --noEmit          # Frontend
cd tauri-app/src-tauri && cargo check     # Backend

# Tests
cd tauri-app && pnpm test:run             # Frontend (Vitest, ~552 passing)
cd tauri-app/src-tauri && cargo test      # Backend (~946 tests, ~29 ignored)

# E2E (requires live STT + LLM Router)
cd tauri-app/src-tauri && cargo test e2e_ -- --ignored --nocapture

# Daily preflight
./scripts/preflight.sh                    # Quick (~10s)
./scripts/preflight.sh --full             # Full (~30s)

# Profile service
cd profile-service && cargo check          # Type check
cd profile-service && cargo test           # Tests

# Mobile processing CLI
cd tauri-app/src-tauri && cargo check --bin process_mobile   # Type check
cd tauri-app/src-tauri && cargo run --bin process_mobile -- --once  # Process one job

# iOS app
cd ios && xcodegen generate                # Regenerate Xcode project
./ios/scripts/test.sh                      # Build + test (60 tests)
./ios/scripts/test.sh --build-only         # Build only

# Release (triggers auto-update for all rooms)
# Bump version in tauri.conf.json + package.json + Cargo.toml, then:
git tag v0.10.11
git push origin main --tags
```

## Auto-Update System

- Tauri updater plugin with Ed25519 signing
- GitHub Releases workflow (`.github/workflows/release.yml`)
- On `v*` tag push: build, sign, create release with `latest.json`
- Running apps check on launch, show blue update banner
- Signing keys stored as GitHub secrets: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

## Mobile App (House Call Recording)

Three-component architecture for offline mobile recording:

1. **iOS App** (`ios/`) — SwiftUI recorder, records AAC audio offline, auto-uploads to profile service on network
2. **Profile Service** — stores audio + tracks job status via `/mobile/*` endpoints (no processing logic)
3. **Processing CLI** (`tauri-app/src-tauri/src/bin/process_mobile.rs`) — polls for jobs, runs STT→encounter detection→SOAP using the same shared Rust modules as the desktop app (zero algorithm divergence)

```bash
# Run processing daemon on MacBook
cargo run --bin process_mobile -- --profile-service-url http://localhost:8090

# Run once and exit
cargo run --bin process_mobile -- --once
```

Design spec: `docs/superpowers/specs/2026-04-13-mobile-app-v1-design.md`

## Detailed Context

See **[tauri-app/CLAUDE.md](tauri-app/CLAUDE.md)** for full architecture, IPC commands, code patterns, gotchas, and troubleshooting.
