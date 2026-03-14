# Backend Analysis

## Executive view
The Rust backend is where the app's real power lives. It already contains the pieces of a serious clinical capture engine: pipeline management, session state, continuous encounter detection, local archiving, Medplum integration, AI routing, vision support, and an MCP service.

The backend's main problem is not capability. It is concentration. A few modules are carrying too many conceptual responsibilities.

Primary hotspots:
- `tauri-app/src-tauri/src/continuous_mode.rs:272`
- `tauri-app/src-tauri/src/llm_client.rs`
- `tauri-app/src-tauri/src/medplum.rs`
- `tauri-app/src-tauri/src/config.rs`
- `tauri-app/src-tauri/src/local_archive.rs`

## What is structurally sound

### 1. Command surface is organized by domain
`src-tauri/src/commands/mod.rs:1` groups commands into separate modules rather than one giant file. That is the right baseline.

### 2. Session state machine is clean and understandable
`src-tauri/src/session.rs:54` is compact and legible.

It correctly separates:
- lifecycle state
- transcript segments
- pending count
- audio output path
- session correlation id

This is one of the cleaner cores in the repo.

### 3. Pipeline config is a real translation layer
`src-tauri/src/pipeline.rs:158` builds `PipelineConfig` from persistent config plus mode-specific overrides. That is a good pattern.

### 4. Config validation is serious enough to build on
`src-tauri/src/config.rs:431` validates ranges and cross-field rules.
`src-tauri/src/config.rs:735` clamps dangerous values on load.
`src-tauri/src/commands/settings.rs:12` validates before save.

This means the backend already has a credible settings core. The frontend is the part that currently undermines it.

## Main backend problems

### 1. `continuous_mode.rs` is a subsystem disguised as a file
This file is `3,371` lines and effectively owns:
- long-running capture lifecycle
- transcript buffer management
- silence-trigger logic
- LLM encounter detection
- sensor mode
- hybrid mode
- shadow mode
- shadow CSV logging
- encounter merge-back
- clinical-content checks
- SOAP generation
- archive persistence
- screenshot analysis tie-ins
- event emission and stats updates

Signals from the file itself:
- `6` `tokio::spawn` call sites
- `25` event `emit(...)` calls
- active branching for `llm`, `sensor`, `hybrid`, and `shadow`
- manual trigger support and notes sync in the same subsystem

Implementation anchors:
- handle and stats: `src-tauri/src/continuous_mode.rs:117`
- core runtime entry: `src-tauri/src/continuous_mode.rs:272`
- sensor startup: `src-tauri/src/continuous_mode.rs:463`
- shadow mode: `src-tauri/src/continuous_mode.rs:596`
- active LLM detection path: `src-tauri/src/continuous_mode.rs:870`
- archive save path: `src-tauri/src/continuous_mode.rs:1508`

This is the single biggest architecture risk in the repo.

Recommendation:
Break it into explicit services:
- `continuous_runtime`
- `encounter_detector`
- `sensor_bridge`
- `shadow_evaluator`
- `encounter_archiver`
- `continuous_soap_worker`
- `continuous_events`

Do not start by over-abstracting traits. Start by extracting cohesive data types and pure decision functions.

### 2. Shutdown is not actually solved
`src-tauri/src/lib.rs:427` tries a graceful pipeline shutdown, but if it succeeds it still forces process exit with `unsafe { libc::_exit(0) }` at `src-tauri/src/lib.rs:474` due to ONNX cleanup crashes.

This is not a cosmetic wart. It means runtime ownership and destructor behavior are not trustworthy yet.

Recommendation:
- Treat model/runtime teardown as a dedicated engineering track.
- Isolate ONNX-backed resources behind explicit shutdown boundaries.
- Consider process-level worker isolation for crash-prone inference runtimes if graceful teardown remains unreliable.

### 3. Local archive is powerful but acting like a mini database without database ergonomics
`src-tauri/src/local_archive.rs:1` provides real production persistence. It handles:
- save session
- list dates
- list sessions
- get details
- add SOAP
- split
- merge
- rename patient
- renumber encounters
- transcript line access

This is pragmatic and useful. It also means the file system is carrying database-like behavior:
- mutable encounter identifiers over time
- merge/split lineage without a formal model
- business logic coupled to directory layout
- no explicit indexing layer

Recommendation:
- Keep the file-based archive if you want portability and simplicity.
- Add an index layer or journal rather than pushing more semantics into folder traversal and file rewrites.
- Model lineage explicitly: `encounter_created_from`, `split_from`, `merged_from`, `renumbered_at`.

### 4. Storage semantics are conceptually mixed
There are at least three persistence concepts in play:
- durable config: `src-tauri/src/config.rs:703`
- durable local archive: `src-tauri/src/local_archive.rs:3`
- debug storage: referenced in session and SOAP paths, such as `src-tauri/src/commands/session.rs:144` and `src-tauri/src/commands/ollama.rs:247`

The main backend issue is not the existence of debug storage. It is that its meaning overlaps with product features and leaks into frontend behavior.

Recommendation:
- Make storage roles explicit in type and naming:
  - `ArchiveStore`
  - `DiagnosticsStore`
  - `SyncStore`
- Stop using debug flags as product feature switches.

### 5. Environment-specific defaults are still baked into runtime config
Current defaults include hardcoded service addresses:
- Whisper server: `src-tauri/src/config.rs:331`
- MIIS server: `src-tauri/src/config.rs:262`
- Pipeline default Whisper server: `src-tauri/src/pipeline.rs:243`

This makes sense for a clinic-specific deployment, but it reduces portability and makes tests/environment bootstrap more fragile.

Recommendation:
- Introduce explicit deployment profiles.
- Treat clinic-local defaults as one profile, not the global app default.

### 6. Command surface is broad but unstructured from a contract perspective
The app exposes `91` Tauri commands. `src-tauri/src/lib.rs:302` registers them all directly. The surface area is broad enough that contract management should now be treated as a platform concern.

Recommendation:
- Generate a command manifest or shared schema.
- Group commands into stable domains with versionable payloads.
- Stop relying on manual shape mirroring in TypeScript.

### 7. The MCP server is strategically interesting but operationally isolated
`src-tauri/src/mcp/server.rs:31` starts an HTTP server on port `7101` and exposes a tool layer for health, status, and logs.

This is a meaningful platform move because it turns the app into an inspectable runtime, not just a desktop UI.

Current limitation:
- It is operationally interesting, but not yet deeply integrated with the rest of the architecture as a managed subsystem.

Recommendation:
- Keep it.
- Expand it only after storage/contracts/continuous mode are stabilized.
- Use it as an observability and operator-control surface, not as a shortcut around missing internal APIs.

## Backend subsystems worth protecting

### Session + pipeline split
- Session state machine: `src-tauri/src/session.rs:54`
- Pipeline message stream: `src-tauri/src/pipeline.rs:70`
- Session command orchestration: `src-tauri/src/commands/session.rs:16`

This separation is correct. Keep it.

### Config validation and atomic save
- Validation: `src-tauri/src/config.rs:435`
- Atomic save: `src-tauri/src/config.rs:789`

This is already better than many apps at this stage.

### Encounter-detection parsing discipline
`src-tauri/src/encounter_detection.rs:130` and `src-tauri/src/encounter_detection.rs:204` show real defensive parsing against LLM formatting errors.

That discipline should be reused across all AI tasks.

## Backend target state
A strong next-stage backend would look like this:
- Tauri commands stay thin
- Runtime services own long-running workflows
- Continuous mode is decomposed into explicit services and data models
- Archive behavior is backed by an index or journal
- Config and environment profiles are explicit
- IPC contracts are generated rather than hand-maintained
- ML runtime shutdown is controlled rather than bypassed with `_exit(0)`

That would preserve the app's unusual power while making it evolvable.
