# AMI Assist

Ambient Medical Intelligence — a real-time speech-to-text transcription desktop application built with Tauri v2, React, and Rust. Designed as a clinical ambient scribe for physicians, running as a compact sidebar alongside EMR systems.

## Features

### Core
- **Two charting modes** — `session` (per-encounter manual start/stop, default for fresh installs) and `continuous` (records all day with auto-detection); selectable per-physician via settings
- **Real-time streaming transcription** via STT Router (WebSocket, medical-optimized aliases)
- **Manual audio upload** — upload an audio file (mp3/wav/m4a/aac/flac/ogg/wma/webm) and run it through the same continuous-mode pipeline (ffmpeg → STT batch → encounter detection → SOAP)
- **Speaker diarization** — ONNX-based speaker embeddings with online clustering
- **Speaker profile sync** — enrolled speaker profiles auto-sync across all rooms at startup
- **Auto-update** — GitHub Releases with Ed25519 signing; rooms detect new versions on launch

### Clinical
- **SOAP note generation** — AI-powered via OpenAI-compatible LLM router with explicit S/O/A/P section definitions
- **Multi-patient SOAP** — supports up to 4 patients per visit with auto patient/physician detection
- **FHO+ Billing Engine** — two-stage extraction (LLM clinical features → deterministic OHIP rule engine); 234 OHIP codes; auto K013A→K033A overflow at 4+ counselling units; per-patient billing; diagnostic-code cross-validation
- **Patient handout** — plain-language visit summary (5th–8th grade reading level); included as context in SOAP generation
- **Differential diagnosis** — top 3 DDx with cardinal symptoms, refreshed every 30s during continuous mode
- **Clinical assistant chat** — `clinical-assistant` LLM alias for live questions during recording; chat history persisted per session (continuous mode)
- **AI medical illustrations** — Gemini-generated images from clinical concepts (default); MIIS server alternative
- **Vision-based patient name + DOB** — screenshot capture + vision LLM to extract patient name and DOB from EMR chart; DOB auto-populates billing age bracket
- **Screenshot archival** — all screenshots saved per encounter for replay and debugging
- **Encounter history** — session browser with sort (time, encounter, patient, words, duration) and filter (clinical, SOAP status)
- **Session cleanup tools** — delete, split, merge sessions, rename patients, renumber encounters; merge is async + crash-safe (atomic rename, server delete awaited)

### Multi-Room Clinic Deployment

| Machine | Role | IP | User |
|---------|------|----|------|
| MacBook | Server (profile service :8090, STT :8001, LLM :8080, Medplum :8103) | 100.119.83.76 | arash |
| iMac | Room 2 workstation | 100.74.186.113 | room2 (pw: 1278) |
| Room 6 Mac | Room 6 workstation | local | backoffice |

- **Profile service** — centralized physician profiles, room config, session storage, speaker enrollments, mobile job tracking
- **Settings merge chain** — hard-coded defaults -> server infrastructure -> server room -> local config -> physician overlay
- **Fire-and-forget sync** — sessions uploaded to server after each encounter, 30s delayed re-sync for late-written files
- **Speaker profiles** — auto-synced bidirectionally at startup (name-based matching, server wins on newer `updated_at`)

### Mobile House Call Support
- **iOS app** (`ios/`) — SwiftUI offline recorder, auto-uploads AAC audio to profile service on network
- **Processing CLI** (`src-tauri/src/bin/process_mobile.rs`) — shares desktop app's Rust modules for zero algorithm divergence
- **Pipeline**: ffmpeg transcode → STT Router → encounter detection → SOAP generation → upload to profile service
- Mobile-recorded sessions appear in desktop History automatically (same profile service storage)

### Presence Sensor (Hybrid Detection)
- **ESP32 multi-sensor bridge** — mmWave radar (SEN0395), thermal camera (MLX90640), CO2/temp/humidity (SCD41)
- **Detection mode auto-derived** — if room has sensor configured (WiFi or USB), uses hybrid detection (sensor + LLM); otherwise LLM-only
- **Sensor tuning** — absence threshold, debounce filter, hybrid confirm window, per-room calibration (thermal, CO2)
- **Connection types** — WiFi (HTTP to ESP32), USB-UART (serial to mmWave; XIAO ESP32-C3 supported), configured per room in admin panel
- **Sleep mode** — auto-pauses continuous mode 10 PM – 6 AM EST (configurable, DST-safe via chrono-tz); sleep banner in UI

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
# Bump version in tauri.conf.json + package.json + src-tauri/Cargo.toml, then:
git tag v0.10.34
git push origin main --tags
# GitHub Actions builds, signs, and publishes to Releases
# All running rooms detect the update on next launch
```

The release workflow (`.github/workflows/release.yml`) builds the app, creates a signed `.tar.gz` bundle, generates `latest.json`, and uploads to GitHub Releases. The Tauri updater plugin checks this endpoint on launch.

## Settings

The app settings are simplified into a flat list:

1. **Continuous Mode** toggle (compiled default is session mode; physicians who prefer continuous flip the toggle once and the choice persists in their physician profile)
2. **Microphone** selector
3. **SOAP Preferences** — personal instructions per physician
4. **Session Automation** — auto-start on greeting, auto-end on silence (session mode only)
5. **Room** — current room name + change button
6. **Speaker Profiles** — manage enrolled voices

Infrastructure settings (STT/LLM URLs, API keys, model aliases) are managed centrally via the profile service and merged at startup. Server-configurable data (LLM prompt templates, billing rules, detection thresholds) follows a three-tier fallback: server fetch → local cache → compiled defaults. Sensor settings are configured per room in the admin panel.

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

See [docs/TESTING.md](../docs/TESTING.md) for the full test architecture (unit, integration, E2E, replay regression layers).

```bash
# Frontend (Vitest)
pnpm test:run

# Rust (cargo test)
cd src-tauri && cargo test --lib

# E2E (requires STT + LLM Router running)
cd src-tauri && cargo test e2e_ -- --ignored --nocapture

# Preflight — runs all 7 layers including offline replay regressions
./scripts/preflight.sh --full

# Offline replay regression (no LLM needed) — runs against ~195 archived bundles
cd src-tauri && cargo run --bin detection_replay_cli -- --all --fail-on-mismatch --threshold 99.0

# Labeled regression (offline) — compares production billing.json to ground-truth labels
cd src-tauri && cargo run --bin labeled_regression_cli -- --all
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
