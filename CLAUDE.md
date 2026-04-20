# AMI Assist

Ambient Medical Intelligence — clinical ambient scribe for physicians. Real-time speech-to-text transcription desktop app with automated encounter detection, SOAP note generation, and multi-room clinic deployment.

## Repository Structure

```
transcriptionapp/
├── tauri-app/              # Desktop application (Tauri v2 + React + Rust)
│   ├── src/                # React + TypeScript frontend
│   ├── src-tauri/
│   │   ├── src/            # Rust library + main binary
│   │   ├── src/bin/        # process_mobile (mobile audio processing daemon)
│   │   ├── tools/          # 12 replay/regression CLIs (detection_replay, merge_replay, clinical_replay, multi_patient_replay, multi_patient_split_replay, benchmark_runner, labeled_regression, golden_day, bootstrap_labels, replay_bundle_backfill, encounter_experiment, vision_experiment)
│   │   ├── benches/        # Criterion benchmarks (audio_benchmarks)
│   │   └── tests/fixtures/ # benchmarks/*.json + labels/*.json (ground-truth corpus)
│   ├── CLAUDE.md           # Detailed codebase context (architecture, commands, patterns)
│   ├── CONTRIBUTING.md     # Development workflow
│   └── README.md           # App-level docs
├── profile-service/        # Standalone axum REST API for multi-user management
│   ├── src/                # Physician profiles, rooms, sessions, speaker enrollments, mobile jobs, server-configurable prompts/billing/thresholds
│   └── CLAUDE.md           # Profile service architecture and patterns
├── ios/                    # iOS mobile app (SwiftUI) — offline recorder + auto-upload
│   ├── AMI Assist/         # Source (Views, Services, Models)
│   └── project.yml         # XcodeGen project spec
├── esp32-presence/         # ESP32 sensor firmware (PlatformIO)
├── room6-xiao-sensor/      # XIAO ESP32-C3 mmWave sensor firmware (Arduino)
├── ops/                    # Server-side ops scripts (auto-deploy, backups, health monitor)
├── .github/workflows/      # CI + Release (auto-update via GitHub Releases)
├── docs/                   # Test architecture, benchmarks, design specs, ADRs
│   ├── TESTING.md          # Authoritative test infrastructure doc (7 layers, replay tools, labeled corpus)
│   ├── benchmarks/         # Per-task benchmark fixture documentation
│   └── superpowers/specs/  # Feature design specs
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
| Profile Service | 8090 | Physician profiles, rooms, sessions, speaker sync, mobile job tracking, server-configurable prompts/billing/thresholds |
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
cd tauri-app && pnpm test:run             # Frontend (Vitest, 594 passing across 32 files)
cd tauri-app/src-tauri && cargo test --lib   # Backend lib (1,170 passing, 30 ignored)
cd tauri-app/src-tauri && cargo test --test harness_per_encounter  # Per-encounter snapshot harness (10 seed bundles)

# E2E (requires live STT + LLM Router)
cd tauri-app/src-tauri && cargo test e2e_ -- --ignored --nocapture

# Daily preflight (7 layers: 5 E2E + detection replay + golden day)
./scripts/preflight.sh                    # Quick (~10s)
./scripts/preflight.sh --full             # Full (~30s)

# Replay regression CLIs (offline + online, see docs/TESTING.md)
cd tauri-app/src-tauri && cargo run --bin detection_replay_cli -- --all --fail-on-mismatch
cd tauri-app/src-tauri && cargo run --bin labeled_regression_cli -- --all
cd tauri-app/src-tauri && cargo run --bin benchmark_runner -- --all --trials 3

# Profile service
cd profile-service && cargo check          # Type check
cd profile-service && cargo test           # Tests (66 passing across integration + lib)

# Mobile processing CLI
cd tauri-app/src-tauri && cargo check --bin process_mobile   # Type check
cd tauri-app/src-tauri && cargo run --bin process_mobile -- --once  # Process one job

# iOS app
cd ios && xcodegen generate                # Regenerate Xcode project
./ios/scripts/test.sh                      # Build + test (60 tests)
./ios/scripts/test.sh --build-only         # Build only

# Release (triggers auto-update for all rooms)
# Bump version in tauri.conf.json + package.json + src-tauri/Cargo.toml, then:
git tag v0.10.46   # use the next patch version
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
3. **Processing CLI** (`tauri-app/src-tauri/src/bin/process_mobile.rs`) — polls for jobs, runs STT→encounter detection→SOAP using shared `audio_processing` module + `transcription_app_lib` (zero algorithm divergence with desktop)

```bash
# Run processing daemon on MacBook
cargo run --bin process_mobile -- --profile-service-url http://localhost:8090

# Run once and exit
cargo run --bin process_mobile -- --once
```

Design spec: `docs/superpowers/specs/2026-04-13-mobile-app-v1-design.md`

## Manual Audio Upload (Desktop)

Same pipeline as mobile, but invoked from the desktop UI. "Upload Recording" link in both ReadyMode and ContinuousMode opens an `AudioUploadModal` where the user picks an audio file (mp3/wav/m4a/aac/flac/ogg/wma/webm) and a date. Backend uses the same `audio_processing` module as `process_mobile` (ffmpeg → STT batch → encounter detection → SOAP). Sessions are written to the local archive under the user-selected date with `charting_mode = "upload"`. Progress events stream to the UI via `audio_upload_progress` Tauri events.

## Server-Configurable Data

Four categories of operational data are pushed centrally without app rebuilds (Phase 1 + 2 + 3, all live):
- **Prompt templates** — `PUT /config/prompts`. LLM prompt builders accept `Option<&PromptTemplates>`.
- **Billing data** — `PUT /config/billing`. Rule engine accepts `Option<&BillingData>`.
- **Detection thresholds** — `PUT /config/thresholds`. Populated on `DetectionEvalContext.server_thresholds` at continuous-mode start; also covers Cat A algorithm constants (vision K/cap, multi-patient detect, screenshot grace, Gemini timeout, `detection_prompt_max_words`).
- **Operational defaults** — `PUT /config/defaults` (`OperationalDefaults`: sleep hours, thermal/CO2 baselines, encounter intervals, 4 model aliases). Precedence: `compiled default < server < local (if user-edited)`, tracked via `Settings.user_edited_fields: Vec<String>` so compiled-default drift can't silently stomp workstations.

Three-tier fallback unchanged: server fetch (startup + version-bump poll) → `~/.transcriptionapp/server_config_cache.json` → compiled defaults. `SharedServerConfig` (Arc<RwLock>) in Tauri managed state. Full detail in `tauri-app/docs/adr/0023-server-configurable-data.md`.

Resolver lives in `tauri-app/src-tauri/src/server_config_resolve.rs` (`resolve()`, `resolve_operational()`, `resolve_effective_models()`); `get_operational_defaults` and `clear_user_edited_field` Tauri commands + the `useOperationalDefaults` frontend hook drive the "Clinic default: X / Reset to clinic default" UI in SettingsDrawer.

## Auto-Deploy (Profile Service)

The profile service on the MacBook is auto-deployed via launchd. The `com.fabricscribe.profile-service-updater` plist runs `~/transcriptionapp-deploy.sh` on a schedule, which pulls the repo, rebuilds `profile-service`, and restarts via launchctl. Logs in `~/transcriptionapp-deploy.log`. See `ops/README.md`.

## Operational observability (v0.10.36+)

LLM calls emit per-call `CallMetrics` (wall_ms, scheduling_ms, network_ms, concurrent_at_start, retry_count) that's folded into each `pipeline_log.jsonl` event's context. At `continuous_mode_stopped` a `performance_summary.json` is written to `archive/YYYY/MM/DD/` with per-step latency percentiles, failure counts, and the scheduling-vs-network split. Makes tail attribution ("is the LLM slow or is our async runtime sleeping?") a one-file lookup instead of a per-session parse. Coverage: encounter_detection, billing_extraction, encounter_merge, clinical_content_check, vision_extraction. Not yet migrated: soap_generation, multi_patient_detect (go through higher-level client wrappers).

## Vision early-stop (v0.10.37, throttle + DOB invalidation in v0.10.45)

Screenshot task skips vision LLM calls once `PatientNameTracker` has K=5 consecutive matching votes (or cap=30 total per encounter). Screenshots still captured + archived for audit. Calibrated from the Apr 16 Room 6 audit: ~78% vision-call reduction (329 → ~70/day) with no downstream behavior change on stable encounters.

**v0.10.45 guards** against mid-encounter chart switches (Apr 20 Room 2 Shelley/Richard root cause):

1. **Re-sample throttle**: even after early-stop fires, vision is re-sampled every `vision_re_sample_interval_secs` (default 600s / 10 min). A chart switch during a 40-minute encounter is detected within one interval instead of locked to the pre-switch name.
2. **DOB invalidation**: when the vision-extracted DOB changes mid-encounter, name votes + streak are cleared — the EMR clearly switched patients, so the old majority no longer applies. `vision_calls_attempted` and `last_vision_call_at` are preserved so the per-encounter cap still bounds budget across the invalidation.

## Forward-merge cleanup (v0.10.43+)

Runs after an encounter splits and the merge-back coordinator returns `Separate`. If the PREVIOUS encounter had a pre-SOAP multi-patient split and one of its sub-SOAPs clinically matches the CURRENT encounter's primary SOAP, rewrites the previous session as single-patient. Fixes the "next patient's check-in audio leaks into prev session's tail → false multi-patient split" failure mode seen on Apr 20 Room 6 (Scott's session captured Cathy's intake at the reception desk; multi-patient detection fired producing a Cathy sub-SOAP in Scott's session alongside Cathy's own session).

Rule: overlap-coefficient of A/P-section clinical terms ≥ 0.30, shared distinctive terms ≥ 5, audio gap (last non-doctor `end_ms` in prev → first non-doctor `start_ms` in curr) ≤ 300s. Audio gap is the load-bearing signal — wall-clock `ended_at`/`started_at` can be rewritten post-hoc by orphan recovery, but per-session audio timestamps are monotonic.

Module: `continuous_mode_forward_merge.rs`. Emits `ForwardMergeFired` event and triggers `resync_session` on the cleaned-up prev session. Validated by 3-day replay simulation (Apr 16/17/20): 3 multi-patient sessions, 1 true positive, 0 false positives across 18 evaluated pairs.

## Detailed Context

See **[tauri-app/CLAUDE.md](tauri-app/CLAUDE.md)** for full architecture, IPC commands, code patterns, gotchas, and troubleshooting. See **[docs/TESTING.md](docs/TESTING.md)** for the test architecture.
