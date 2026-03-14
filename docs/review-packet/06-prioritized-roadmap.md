# Prioritized Roadmap

## Goal
Take the app from a high-functioning, vibe-coded clinical tool into a durable product platform without flattening the parts that make it interesting.

The roadmap below assumes the current differentiator is the combined system, especially continuous charting, not just session transcription.

## Priority order

### Priority 1: Fix state and contract foundations
Why first:
- This reduces change risk everywhere else
- It improves both AI-assisted coding and human maintenance
- It removes a large amount of invisible product ambiguity

Deliverables:
1. Typed IPC gateway on the frontend
- Replace raw `invoke(...)` calls with domain-specific wrappers
- Start with settings, session, history, continuous mode, and Medplum

2. Explicit settings semantics
- Split settings into `live`, `draft`, and `test-scope`
- Remove all "persist just to test" behavior
- Keep backend validation untouched

3. Contract coverage for critical payloads
- `Settings`
- `ContinuousModeStats`
- history/detail payloads
- screen capture status
- Medplum sync responses

Code anchors:
- `tauri-app/src/hooks/useSettings.ts:87`
- `tauri-app/src/hooks/useConnectionTests.ts:107`
- `tauri-app/src/hooks/useOllamaConnection.ts:84`
- `tauri-app/src/types/index.ts:53`
- `tauri-app/src/contracts.test.ts:1`

Success criteria:
- No feature mutates persistent settings accidentally
- Frontend stops using raw command strings in most feature code
- Contract regressions fail clearly and early

### Priority 2: Refactor the frontend around domains, not one root orchestrator
Why second:
- Feature work is currently bottlenecked by a few very large files
- This is the fastest way to make future AI-generated edits safer

Deliverables:
1. Split `App.tsx` into shell controllers
- `SessionShell`
- `ContinuousShell`
- `SettingsController`
- `HistoryLauncher`

2. Extract shared SOAP workspace components
- shared note renderer/editor
- shared per-patient tabs
- shared copy/regenerate/save actions

3. Move history logic into a dedicated history domain layer
- separate data source policy from UI
- treat local archive as a first-class production source

Code anchors:
- `tauri-app/src/App.tsx:42`
- `tauri-app/src/components/HistoryWindow.tsx:55`
- `tauri-app/src/components/modes/ReviewMode.tsx:89`
- `tauri-app/src/components/SettingsDrawer.tsx:57`

Success criteria:
- `App.tsx` becomes a thin shell
- Review and history no longer implement separate SOAP systems
- History source selection is product-correct

### Priority 3: Break up continuous mode into explicit backend services
Why third:
- This is the app's hardest and most differentiated subsystem
- It is also the biggest drag on confidence and iteration speed

Deliverables:
1. Extract encounter decision pipeline from `continuous_mode.rs`
- transcript buffering
- split detection
- merge-back
- clinical-content filtering
- archival handoff

2. Separate detection strategies
- LLM strategy
- sensor strategy
- hybrid coordinator
- shadow evaluator

3. Separate persistence and note-generation from runtime coordination
- `EncounterArchiver`
- `ContinuousSoapWorker`
- `ContinuousEventEmitter`

Code anchors:
- `tauri-app/src-tauri/src/continuous_mode.rs:272`
- `tauri-app/src-tauri/src/encounter_detection.rs:52`
- `tauri-app/src-tauri/src/encounter_merge.rs`
- `tauri-app/src-tauri/src/local_archive.rs:179`

Success criteria:
- Continuous mode can be reasoned about by subsystem
- Core split/merge decisions can be tested without running the full runtime
- Archive writes are clearly downstream of encounter decisions

### Priority 4: Formalize the storage model
Why fourth:
- The app is already storing durable clinical work locally
- Merge/split/renumber behavior has outgrown an implicit file-layout model

Deliverables:
1. Define storage roles explicitly
- archive store
- diagnostics store
- sync state store

2. Add archive index/journal
- encounter lineage
- split/merge history
- stable identifiers vs display numbering

3. Uncouple history UI from debug storage semantics

Code anchors:
- `tauri-app/src-tauri/src/local_archive.rs:1`
- `tauri-app/src-tauri/src/commands/archive.rs:1`
- `tauri-app/src/components/HistoryWindow.tsx:71`

Success criteria:
- History semantics are stable and explainable
- Cleanup operations preserve lineage
- Local archive becomes a durable product primitive rather than a convenient file tree

### Priority 5: Build an AI task platform around existing capabilities
Why fifth:
- The app already depends on many AI tasks
- The leverage now comes from making them measurable and composable

Deliverables:
1. Shared AI task executor abstraction
2. Per-task schemas, timeouts, and routing policies
3. Evaluation fixtures for:
- encounter splitting
- merge-back
- multi-patient SOAP
- predictive hint quality
- vision SOAP usefulness

4. Consistent logging for every AI task

Code anchors:
- `tauri-app/src-tauri/src/commands/ollama.rs:165`
- `tauri-app/src-tauri/src/commands/ollama.rs:314`
- `tauri-app/src-tauri/src/commands/ollama.rs:420`
- `tauri-app/src-tauri/src/commands/clinical_chat.rs:66`
- `tauri-app/src-tauri/src/encounter_detection.rs:204`

Success criteria:
- AI behavior is observable and replayable
- Model-routing decisions are explicit
- Continuous mode can improve through evaluation, not just tuning by feel

### Priority 6: Stabilize runtime lifecycle and environment portability
Why sixth:
- This is necessary for wider deployment confidence
- It is probably not the best first move unless shutdown pain is already severe in daily use

Deliverables:
1. Remove forced `_exit(0)` shutdown path if possible
2. Introduce deployment profiles for service URLs and clinic defaults
3. Make backend integration tests hermetic

Code anchors:
- `tauri-app/src-tauri/src/lib.rs:427`
- `tauri-app/src-tauri/src/config.rs:262`
- `tauri-app/src-tauri/src/config.rs:331`
- `tauri-app/src-tauri/src/pipeline.rs:243`

Success criteria:
- Clean shutdown without runtime crash workarounds
- Fewer machine-specific defaults in core config
- Backend tests fail only on real regressions

## Suggested execution plan by horizon

### Horizon 1: 1-2 weeks
- Fix settings semantics
- Add typed IPC gateway for the most-used domains
- Correct history source behavior
- Fix screenshot count semantics
- Reduce frontend test warning noise

### Horizon 2: 2-6 weeks
- Split `App.tsx`
- Extract shared SOAP workspace
- Start continuous-mode service extraction
- Make local archive tests hermetic

### Horizon 3: 6-12 weeks
- Complete continuous-mode decomposition
- Add archive index/journal
- Build AI evaluation harness
- Revisit shutdown/runtime lifecycle

## What not to do
- Do not start with a full rewrite.
- Do not introduce generic abstractions before the boundaries are obvious.
- Do not add more product surfaces on top of the current orchestration hotspots without first reducing coordination density.
- Do not mistake passing tests for clean contracts. The current suites prove useful coverage, not architecture clarity.

## Highest-leverage first tickets
1. Create `src/lib/ipc/` and move 5-10 highest-traffic commands behind typed wrappers.
2. Add non-persisting backend commands for connection testing.
3. Change history source selection from `debug_storage_enabled` to an explicit policy.
4. Fix screenshot counting so it reflects captures, not files.
5. Split SOAP UI into shared components used by review and history.
6. Extract the first pure decision layer from `continuous_mode.rs`.

## Strategic question worth answering next
The code suggests the product wants to become a continuous clinical workflow system. The one question that would sharpen the roadmap most is:

- Is the flagship experience meant to be "best-in-class single visit scribing" or "ambient all-day encounter intelligence"?

The codebase already leans toward the second. If that is the real vision, architecture and evaluation should start optimizing for it explicitly.
