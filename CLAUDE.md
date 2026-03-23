# AMI Assist

Ambient Medical Intelligence — clinical ambient scribe for physicians. Real-time speech-to-text transcription desktop app with automated encounter detection, SOAP note generation, and multi-room clinic deployment.

## Repository Structure

```
transcriptionapp/
├── tauri-app/              # Main application (all source code lives here)
│   ├── src/                # React + TypeScript frontend
│   ├── src-tauri/          # Rust + Tauri v2 backend
│   ├── CLAUDE.md           # Detailed codebase context (architecture, commands, patterns)
│   ├── CONTRIBUTING.md     # Development workflow
│   └── README.md           # App-level docs
├── profile-service/        # Standalone axum REST API for multi-user management
│   └── src/                # Physician profiles, rooms, sessions, speaker enrollments
├── .github/workflows/      # CI + Release (auto-update via GitHub Releases)
├── docs/                   # Historical review documents, benchmarks, plans
│   └── *.md                # Code review docs (ADRs live in tauri-app/docs/adr/)
├── scripts/                # Build and preflight scripts
└── CODE_REVIEW_FINDINGS.md # Review findings tracker
```

## Clinic Deployment

| Machine | Role | IP (Tailscale) | User | Location |
|---------|------|----------------|------|----------|
| MacBook | Server (profile service, STT Router, LLM Router, Medplum) | 100.119.83.76 | arash | Server room |
| iMac | Room 2 clinic workstation | 100.74.186.113 | room2 (password: 1278) | Room 2 |
| This computer | Room 6 clinic workstation | (local) | backoffice | Room 6 |

### Services on MacBook (100.119.83.76)

| Service | Port | Purpose |
|---------|------|---------|
| Profile Service | 8090 | Physician profiles, rooms, sessions, speaker sync |
| STT Router | 8001 | WebSocket streaming transcription (alias: `medical-streaming`) |
| LLM Router | 8080 | SOAP generation, encounter detection, vision |
| Medplum | 8103 | EMR/FHIR server |

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
cd tauri-app && pnpm test:run             # Frontend (Vitest, ~415 passing)
cd tauri-app/src-tauri && cargo test      # Backend (~764 tests, ~29 ignored)

# E2E (requires live STT + LLM Router)
cd tauri-app/src-tauri && cargo test e2e_ -- --ignored --nocapture

# Daily preflight
./scripts/preflight.sh                    # Quick (~10s)
./scripts/preflight.sh --full             # Full (~30s)

# Release (triggers auto-update for all rooms)
# Bump version in tauri.conf.json + package.json, then:
git tag v0.4.0
git push origin main --tags
```

## Auto-Update System

- Tauri updater plugin with Ed25519 signing
- GitHub Releases workflow (`.github/workflows/release.yml`)
- On `v*` tag push: build, sign, create release with `latest.json`
- Running apps check on launch, show blue update banner
- Signing keys stored as GitHub secrets: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

## Detailed Context

See **[tauri-app/CLAUDE.md](tauri-app/CLAUDE.md)** for full architecture, IPC commands, code patterns, gotchas, and troubleshooting.
