# System Map

## Runtime topology
At runtime the app is composed of five major layers:

1. React/Tauri frontend
2. Tauri command/event bridge
3. Rust runtime services
4. External AI and integration services
5. File-system persistence under `~/.transcriptionapp`

Implementation anchors:
- Frontend boot: `tauri-app/src/main.tsx:1`
- Frontend root: `tauri-app/src/App.tsx:42`
- Rust app setup and command registration: `tauri-app/src-tauri/src/lib.rs:174`
- Command namespace: `tauri-app/src-tauri/src/commands/mod.rs:1`
- Config persistence: `tauri-app/src-tauri/src/config.rs:696`

## Product modes in code
The app operates in two distinct charting models:

1. Session mode
- Start recording
- Stream transcript and biomarkers
- Stop session
- Generate SOAP note
- Optionally sync to Medplum

Primary files:
- `tauri-app/src/App.tsx:160`
- `tauri-app/src/hooks/useSessionState.ts`
- `tauri-app/src/hooks/useSoapNote.ts:63`
- `tauri-app/src-tauri/src/commands/session.rs:16`
- `tauri-app/src-tauri/src/pipeline.rs:112`
- `tauri-app/src-tauri/src/session.rs:54`

2. Continuous mode
- Record indefinitely
- Buffer live transcript
- Detect encounter boundaries with LLM, sensor, hybrid, or shadow logic
- Archive encounters
- Generate SOAP automatically
- Maintain rolling status for the current encounter

Primary files:
- `tauri-app/src/App.tsx:160`
- `tauri-app/src/hooks/useContinuousMode.ts:49`
- `tauri-app/src/hooks/useContinuousModeOrchestrator.ts:71`
- `tauri-app/src-tauri/src/commands/continuous.rs:12`
- `tauri-app/src-tauri/src/continuous_mode.rs:272`
- `tauri-app/src-tauri/src/encounter_detection.rs:52`

## Real product identity
The codebase reveals a product identity that is broader than the repository name.

This is already a "clinical cockpit" with these active subsystems:
- Transcription pipeline: `src-tauri/src/pipeline.rs:41`
- Session state machine: `src-tauri/src/session.rs:54`
- Continuous encounter engine: `src-tauri/src/continuous_mode.rs:272`
- Local archive and session cleanup tooling: `src-tauri/src/local_archive.rs:1`, `src-tauri/src/commands/archive.rs:1`
- SOAP generation: `src-tauri/src/commands/ollama.rs:165`
- Clinical chat: `src/hooks/useClinicalChat.ts:29`, `src-tauri/src/commands/clinical_chat.rs:66`
- Predictive hints and image prompting: `src-tauri/src/commands/ollama.rs:293`
- Screenshot and vision SOAP: `src-tauri/src/screenshot.rs:1`, `src-tauri/src/commands/ollama.rs:420`
- Medplum sync and OAuth: `src/components/AuthProvider.tsx:51`, `src-tauri/src/medplum.rs`
- Speaker enrollment and verification: `src/components/SpeakerEnrollment.tsx:28`, `src-tauri/src/speaker_profiles.rs`
- Presence-sensor logic: `src-tauri/src/presence_sensor.rs`, `src-tauri/src/continuous_mode.rs:463`
- MCP server: `src-tauri/src/mcp/server.rs:31`

That matters because architecture decisions should stop assuming "single feature app" constraints.

## Frontend shape
The frontend is not organized around route-level pages. It is organized around one large root component plus large stateful panes.

Main coordination files:
- `tauri-app/src/App.tsx:42`
- `tauri-app/src/components/SettingsDrawer.tsx:57`
- `tauri-app/src/components/HistoryWindow.tsx:55`
- `tauri-app/src/components/modes/ReviewMode.tsx:89`

Important frontend composition pattern:
- Many domain hooks exist, which is good.
- `App.tsx` still composes and coordinates most of them directly.
- The hooks are often state wrappers around raw IPC rather than true domain boundaries.

## Backend shape
The backend has a cleaner conceptual separation than the frontend, but some subsystems have grown far beyond module-sized boundaries.

Relatively clean boundaries:
- Config: `src-tauri/src/config.rs`
- Session state: `src-tauri/src/session.rs`
- Tauri commands by domain: `src-tauri/src/commands/mod.rs:5`
- Archive commands: `src-tauri/src/commands/archive.rs:1`

Overloaded boundaries:
- Continuous mode: `src-tauri/src/continuous_mode.rs:272`
- LLM client and prompt work: `src-tauri/src/llm_client.rs`
- Medplum integration: `src-tauri/src/medplum.rs`

## Storage model
The storage story is split across three different concepts:

1. Durable config in `~/.transcriptionapp/config.json`
- `src-tauri/src/config.rs:703`
- Atomic save with clamping and validation exists

2. Durable local encounter archive in `~/.transcriptionapp/archive/...`
- `src-tauri/src/local_archive.rs:3`
- This is production behavior, not just debug tooling

3. Debug storage
- Used selectively for extra artifacts
- Still leaks into user-facing logic in at least one place

The core issue is not lack of storage. The issue is conceptual overlap between archive, debug storage, and Medplum-backed history.

## Integration map
External integration points visible in code:
- Whisper/STT server: `src-tauri/src/whisper_server.rs`
- LLM router: `src-tauri/src/commands/ollama.rs:203`
- Medplum: `src-tauri/src/medplum.rs`
- MIIS image server: `src-tauri/src/commands/miis.rs`
- Gemini image generation: `src-tauri/src/commands/images.rs:16`
- Deep link + browser OAuth: `src/components/AuthProvider.tsx:85`, `src-tauri/src/lib.rs:178`
- Presence sensor over serial: `src-tauri/src/commands/continuous.rs:134`
- MCP over HTTP port `7101`: `src-tauri/src/mcp/server.rs:31`

## Architecture strengths
The codebase already has some strong bones:
- The backend command surface is at least domain-grouped rather than dumped into one file.
- `Settings` validation and clamping are real, not superficial. See `src-tauri/src/config.rs:431` and `src-tauri/src/config.rs:735`.
- Session and pipeline are conceptually distinct, which is correct. See `src-tauri/src/session.rs:54` and `src-tauri/src/pipeline.rs:255`.
- Continuous mode exposes status as a handle, which is a good direction even if the implementation is too large. See `src-tauri/src/continuous_mode.rs:117`.

## Architecture weaknesses
The biggest cross-cutting weaknesses are:
- Orchestration concentration in a few files
- Storage semantics that are not consistently modeled in the UI
- Stringly typed IPC without a generated shared contract
- AI task logic embedded directly into product flows instead of living behind explicit domain services
- Environment assumptions baked into defaults and tests

## North-star architecture direction
A better long-term system shape would be:
- Frontend organized around explicit domains: session, continuous encountering, settings, history, sync, assistant
- Shared generated contracts between Rust and TypeScript
- Backend organized around service layers instead of "commands + giant feature modules"
- Storage abstracted as archive store + sync store + debug diagnostics, each with explicit ownership
- AI tasks treated as first-class workflows with evaluation and routing policies

That is the frame for the rest of this analysis pack.
