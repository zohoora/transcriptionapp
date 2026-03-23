# AMI Assist

Ambient Medical Intelligence — a real-time speech-to-text transcription desktop application built with Tauri v2, React, and Rust. Designed as a clinical ambient scribe for physicians, running as a compact sidebar alongside EMR systems.

## Features

### Core
- **Continuous mode** (default) — records all day, auto-detects patient encounters, generates SOAP notes per encounter
- **Real-time streaming transcription** via STT Router (WebSocket, medical-optimized aliases)
- **Speaker diarization** — ONNX-based speaker embeddings with online clustering
- **Speaker profile sync** — enrolled speaker profiles auto-sync across all rooms at startup
- **Auto-update** — GitHub Releases with Ed25519 signing; rooms detect new versions on launch

### Clinical
- **SOAP note generation** — AI-powered via OpenAI-compatible LLM router with explicit S/O/A/P section definitions
- **Multi-patient SOAP** — supports up to 4 patients per visit with auto patient/physician detection
- **AI medical illustrations** — Gemini-generated images from clinical concepts
- **Vision-based patient name** — screenshot capture + vision LLM to extract patient name from EMR chart
- **Screenshot archival** — all screenshots saved per encounter for replay and debugging
- **Encounter history** — session browser with sort (time, encounter, patient, words, duration) and filter (clinical, SOAP status)
- **Session cleanup tools** — delete, split, merge sessions, rename patients, renumber encounters

### Multi-Room Clinic Deployment

| Machine | Role | IP | User |
|---------|------|----|------|
| MacBook | Server (profile service :8090, STT :8001, LLM :8080, Medplum :8103) | 100.119.83.76 | arash |
| iMac | Room 2 workstation | 100.74.186.113 | room2 (pw: 1278) |
| Room 6 Mac | Room 6 workstation | local | backoffice |

- **Profile service** — centralized physician profiles, room config, session storage, speaker enrollments
- **Settings merge chain** — hard-coded defaults -> server infrastructure -> server room -> local config -> physician overlay
- **Fire-and-forget sync** — sessions uploaded to server after each encounter, 30s delayed re-sync for late-written files
- **Speaker profiles** — auto-synced bidirectionally at startup (name-based matching, server wins on newer `updated_at`)

### Presence Sensor (Hybrid Detection)
- **ESP32 multi-sensor bridge** — mmWave radar (SEN0395), thermal camera (MLX90640), CO2/temp/humidity (SCD41)
- **Detection mode auto-derived** — if room has sensor configured (WiFi or USB), uses hybrid detection; otherwise LLM-only
- **Sensor tuning** — absence threshold, debounce filter, hybrid confirm window, per-room calibration (thermal, CO2)
- **Connection types** — WiFi (HTTP to ESP32), USB-UART (serial to mmWave), configured per room in admin panel

### Biomarker Analysis
- **Vitality (prosody)** — pitch variability analysis for affect detection
- **Stability (neurological)** — CPP measurement for vocal control
- **Cough detection** — YAMNet-based audio event classification
- **Conversation dynamics** — turn-taking, overlap, response latency metrics

## Requirements

- Node.js 20+
- Rust 1.70+
- pnpm 10+
- ONNX Runtime (for speaker diarization, enhancement, YAMNet)
- STT Router (required — speech-to-text via WebSocket streaming)
- LLM Router (required — SOAP generation, encounter detection, vision)
- Profile service (port 8090 — multi-user management, session storage)
- Medplum server (optional — EMR integration)

## Quick Start

```bash
# Install dependencies
pnpm install

# Build the app
pnpm tauri build --debug

# Bundle ONNX Runtime
./scripts/bundle-ort.sh "src-tauri/target/debug/bundle/macos/AMI Assist.app"

# Launch
open "src-tauri/target/debug/bundle/macos/AMI Assist.app"
```

**Note**: Use `pnpm tauri build --debug` instead of `tauri dev` — deep links and single-instance plugin don't work in dev mode.

## Release & Auto-Update

```bash
# Bump version in tauri.conf.json + package.json, then:
git tag v0.4.0
git push origin main --tags
# GitHub Actions builds, signs, and publishes to Releases
# All running rooms detect the update on next launch
```

The release workflow (`.github/workflows/release.yml`) builds the app, creates a signed `.tar.gz` bundle, generates `latest.json`, and uploads to GitHub Releases. The Tauri updater plugin checks this endpoint on launch.

## Settings

The app settings are simplified into a flat list:

1. **Continuous Mode** toggle (on by default)
2. **Microphone** selector
3. **SOAP Preferences** — personal instructions per physician
4. **Session Automation** — auto-start on greeting, auto-end on silence (session mode only)
5. **Room** — current room name + change button
6. **Speaker Profiles** — manage enrolled voices

Infrastructure settings (STT/LLM URLs, API keys, model aliases) are managed centrally via the profile service and merged at startup. Sensor settings are configured per room in the admin panel.

## Configuration

Settings stored in `~/.transcriptionapp/config.json`. Room config in `~/.transcriptionapp/room_config.json`.

## File Locations

| File | Location |
|------|----------|
| All models | `~/.transcriptionapp/models/` |
| Settings | `~/.transcriptionapp/config.json` |
| Room config | `~/.transcriptionapp/room_config.json` |
| Speaker profiles | `~/.transcriptionapp/speaker_profiles.json` |
| Session archive | `~/.transcriptionapp/archive/YYYY/MM/DD/session_id/` |
| Screenshots | `~/.transcriptionapp/archive/.../session_id/screenshots/` |
| Activity logs | `~/.transcriptionapp/logs/activity.log.*` |
| Sensor CSV logs | `~/.transcriptionapp/mmwave/` |
| Physician cache | `~/.transcriptionapp/cache/` |

## Testing

```bash
# Frontend (~415 tests)
pnpm test:run

# Rust (~764 tests)
cd src-tauri && cargo test

# E2E (requires STT + LLM Router)
cd src-tauri && cargo test e2e_ -- --ignored --nocapture

# Preflight (verifies all services)
./scripts/preflight.sh --full
```

## Building for Room 2 (iMac)

The iMac has Rust, Node, and pnpm installed. To build directly:

```bash
ssh room2@100.74.186.113  # password: 1278
export PATH="/usr/local/node/bin:$HOME/.cargo/bin:$PATH"
cd ~/transcriptionapp/tauri-app
git pull && pnpm install
pnpm tauri build
./scripts/bundle-ort.sh "src-tauri/target/release/bundle/macos/AMI Assist.app"
cp -R "src-tauri/target/release/bundle/macos/AMI Assist.app" "/Applications/AMI Assist.app"
open "/Applications/AMI Assist.app"
```

Or build on the MacBook (100.119.83.76) and copy the `.app` bundle to the iMac.

## Architecture

See [docs/adr/](./docs/adr/) for Architecture Decision Records, [CLAUDE.md](./CLAUDE.md) for detailed codebase context.

## License

MIT
