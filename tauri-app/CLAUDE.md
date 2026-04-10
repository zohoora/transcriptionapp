# Claude Code Context

AMI Assist (Ambient Medical Intelligence) — clinical ambient scribe for physicians. Real-time speech-to-text transcription desktop app with automated encounter detection, SOAP generation, and multi-room clinic deployment.

## Tech Stack

- **Frontend**: React + TypeScript + Vite
- **Backend**: Rust + Tauri v2
- **Transcription**: STT Router (WebSocket streaming to Whisper backend, alias-based routing)
- **Speaker Detection**: ONNX-based speaker embeddings + online clustering
- **LLM**: OpenAI-compatible API for SOAP note generation
- **EMR**: Medplum FHIR server integration

## Architecture

```
React Frontend (sidebar UI)
       │
       │ IPC (invoke/listen)
       ▼
Rust Backend
├── commands/          # Tauri command handlers
│   ├── mod.rs             # Re-exports, CommandError, PipelineState
│   ├── session.rs         # Recording lifecycle + auto-end archive
│   ├── settings.rs        # get/set settings
│   ├── audio.rs           # Device enumeration
│   ├── models.rs          # Model download commands
│   ├── ollama.rs          # LLM router connection
│   ├── medplum.rs         # EMR sync commands
│   ├── listening.rs       # Auto-session detection commands
│   ├── speaker_profiles.rs # Speaker enrollment CRUD
│   ├── clinical_chat.rs   # Clinical assistant chat proxy
│   ├── miis.rs            # Medical illustration proxy
│   ├── images.rs          # AI image generation (Gemini)
│   ├── screenshot.rs      # Screen capture commands
│   ├── continuous.rs      # Continuous charting mode commands
│   ├── archive.rs         # Local session history commands
│   ├── billing.rs         # FHO+ billing commands (9 commands, incl. search + context toggles + diagnostic codes)
│   ├── whisper_server.rs  # STT Router status commands
│   └── permissions.rs     # Microphone permission commands
├── lib.rs             # Tauri app setup, plugin registration, command routing
├── main.rs            # Binary entry point
├── session.rs         # Recording state machine
├── audio.rs           # Audio capture, resampling
├── vad.rs             # Voice Activity Detection
├── pipeline.rs        # Processing pipeline + silence tracking
├── config.rs          # Settings persistence
├── llm_client.rs      # OpenAI-compatible LLM client
├── medplum.rs         # FHIR client (OAuth, encounters)
├── whisper_server.rs  # STT Router client (WebSocket streaming + batch fallback)
├── transcription.rs   # Segment and utterance types
├── models.rs          # Model download management
├── checklist.rs       # Pre-flight verification system
├── listening.rs       # Auto-session detection
├── speaker_profiles.rs # Speaker enrollment storage
├── local_archive.rs   # Local session storage
├── continuous_mode.rs # Continuous charting mode (end-of-day)
├── presence_sensor/   # Multi-sensor presence detection suite
│   ├── mod.rs             # PresenceSensorSuite orchestrator, fusion task, public API
│   ├── types.rs           # PresenceState, SensorType, SensorReading, FusedState, configs
│   ├── sensor_source.rs   # SensorSource trait
│   ├── sources/
│   │   ├── esp32_http.rs  # HTTP poller — mmWave + CO2 + thermal from ESP32 WiFi bridge
│   │   ├── serial.rs      # USB-UART mmWave (XIAO ESP32-C3 or legacy JYBSS), auto_detect_port(), JSON + JYBSS parsers
│   │   └── mock.rs        # Scripted timelines for testing
│   ├── debounce.rs        # DebounceFsm — filters rapid toggles
│   ├── thermal.rs         # Hot-pixel counting + flood-fill blob detection (pure functions)
│   ├── co2.rs             # Rolling CO2 tracker, trend analysis, occupancy estimation
│   ├── fusion.rs          # Sensor fusion engine (mmWave-only passthrough, multi-sensor deferred)
│   ├── absence_monitor.rs # Absence threshold timer → triggers encounter split
│   └── csv_logger.rs      # Daily-rotating mmWave CSV logs
├── screenshot.rs      # Screen capture (in-memory JPEG, blank detection, permission check)
├── patient_name_tracker.rs # Vision-based patient name + DOB extraction (JSON format) + recency-weighted vote tracker
├── encounter_detection.rs  # Encounter detection prompts/parsing + clinical content check + retrospective multi-patient check
├── encounter_merge.rs # Encounter merge prompts/parsing (M1 name-aware strategy)
├── encounter_pipeline.rs # Shared encounter pipeline helpers (SOAP generation, merge checks, clinical content check)
├── screenshot_task.rs # Screenshot capture task for continuous mode (extracted from continuous_mode.rs)
├── continuous_mode_events.rs # Typed event emission for continuous mode
├── server_sync.rs     # ServerSyncContext — fire-and-forget session upload + billing.json sync
├── shadow_observer.rs # Shadow mode observer task (sensor-side for dual detection comparison)
├── co2_calibration.rs # CO2 sensor baseline calibration tool
├── debug_storage.rs   # Debug storage (dev only)
├── permissions.rs     # macOS permission checks
├── ollama.rs          # Re-exports from llm_client.rs (backward compat)
├── activity_log.rs    # Structured PHI-safe activity logging
├── shadow_log.rs      # Shadow mode CSV logging (dual detection comparison)
├── gemini_client.rs   # Google Gemini API client (image generation)
├── profile_client.rs    # HTTP client for profile service (physicians, sessions, speakers, rooms)
├── room_config.rs       # Room config (room name, profile server URL, room ID)
├── physician_cache.rs   # Local cache for physician list + settings
├── audio_upload_queue.rs # Background audio upload queue for server sync
├── pipeline_log.rs    # Pipeline replay JSONL logger (detection, SOAP, screenshot events)
├── segment_log.rs     # Per-segment JSONL timeline logger (continuous mode)
├── replay_bundle.rs   # Self-contained encounter replay test case builder
├── day_log.rs         # Day-level orchestration JSONL logger
├── transcript_buffer.rs # Timestamped transcript segment buffer (continuous mode)
├── encounter_experiment.rs # Encounter detection experiment CLI support
├── vision_experiment.rs    # Vision SOAP experiment CLI support
├── diarization/       # Speaker detection (ONNX embeddings, clustering)
├── enhancement/       # Speech enhancement (GTCRN)
├── billing/             # FHO+ billing engine (234 OHIP codes + 562 diagnostic codes, SOB-verified)
│   ├── mod.rs               # Module root, re-exports
│   ├── types.rs             # BillingRecord, BillingCode, TimeEntry, cap types
│   ├── ohip_codes.rs        # Static OHIP code database (234 codes, 21 exclusion groups)
│   ├── diagnostic_codes.rs  # OHIP diagnostic codes (562 ICD-8 codes, MOH Apr 2023 + Mar 2026)
│   ├── clinical_features.rs # LLM extraction schema (23 visit types, 79 procedures, 14 conditions)
│   ├── rule_engine.rs       # Deterministic feature → OHIP code mapping (94 codes reachable)
│   └── time_tracking.rs     # Q310-Q313 time calculation, daily/monthly caps
├── biomarkers/        # Vocal analysis (vitality, stability, cough detection)
├── mcp/               # MCP server on port 7101
├── preprocessing.rs   # DC removal, high-pass filter, AGC
├── command_tests.rs   # Unit tests for commands
├── pipeline_tests.rs  # Unit tests for pipeline
├── e2e_tests.rs       # Integration tests (5 layers, #[ignore])
├── soak_tests.rs      # Long-running stability tests
└── stress_tests.rs    # Load/stress tests
```

## Quick Start

```bash
# Build (use debug build, NOT tauri dev - required for OAuth deep links)
pnpm tauri build --debug

# Bundle ONNX Runtime (one-time after build)
./scripts/bundle-ort.sh "src-tauri/target/debug/bundle/macos/AMI Assist.app"

# Run
open "src-tauri/target/debug/bundle/macos/AMI Assist.app"

# Verify
npx tsc --noEmit                 # TypeScript typecheck
cd src-tauri && cargo check      # Rust compile check

# Tests
pnpm test:run                    # Frontend (Vitest)
cd src-tauri && cargo test       # Rust

# Daily preflight (verifies STT + LLM + Archive before clinic)
./scripts/preflight.sh           # Quick (~10s): layers 1-3
./scripts/preflight.sh --full    # Full (~30s): all 5 layers
```

**Why not `tauri dev`?** Deep links and single-instance plugin don't work in dev mode. OAuth callbacks open new instances instead of routing to existing app.

## Key Files for Common Tasks

| Task | Files to Modify |
|------|-----------------|
| Add new setting | `config.rs`, `types/index.ts`; if UI-visible: also `useSettings.ts` (PendingSettings + `buildMergedSettings()`), `SettingsDrawer.tsx` (Zone 1 or Zone 3 Advanced) |
| Modify transcription | `pipeline.rs`, `whisper_server.rs` (STT Router streaming), `transcription.rs` (types) |
| Change SOAP prompt | `llm_client.rs` (`build_multi_patient_soap_prompt()`) |
| Modify LLM integration | `llm_client.rs`, `commands/ollama.rs`, `useOllamaConnection.ts` |
| Add new biomarker | `biomarkers/mod.rs`, `BiomarkersSection.tsx` |
| Modify biomarker thresholds | `types/index.ts` (`BIOMARKER_THRESHOLDS`, status helper functions) |
| Modify UI modes | `components/modes/` (ReadyMode, RecordingMode, ReviewMode) |
| Add Tauri command | `commands/*.rs`, register in `lib.rs` |
| Modify Medplum sync | `commands/medplum.rs`, `useMedplumSync.ts`, `App.tsx` |
| Modify auto-detection | `listening.rs`, `commands/listening.rs`, `useAutoDetection.ts` |
| Modify speaker enrollment | `speaker_profiles.rs`, `commands/speaker_profiles.rs`, `useSpeakerProfiles.ts`, `SpeakerEnrollment.tsx` |
| Modify clinical chat | `commands/clinical_chat.rs`, `useClinicalChat.ts`, `ClinicalChat.tsx` |
| Modify auto-end detection | `pipeline.rs` (silence tracking), `config.rs` (settings), `useSessionState.ts` |
| Modify SOAP options | `useSoapNote.ts` (hook), `llm_client.rs` (prompt building), `local_archive.rs` (metadata) |
| Modify MIIS integration | `commands/miis.rs`, `useMiisImages.ts`, `ImageSuggestions.tsx`, `usePredictiveHint.ts` |
| Modify AI images | `gemini_client.rs`, `commands/images.rs`, `useAiImages.ts`, `usePredictiveHint.ts`, `ImageSuggestions.tsx` |
| Modify continuous mode | `continuous_mode.rs`, `encounter_detection.rs` (detection prompts + retrospective check), `encounter_merge.rs` (merge prompts), `encounter_pipeline.rs` (shared SOAP generation + merge check helpers), `commands/continuous.rs`, `useContinuousMode.ts`, `ContinuousMode.tsx` |
| Modify presence sensor | `presence_sensor/` (module directory), `config.rs` (sensor fields), `commands/continuous.rs`, `SettingsDrawer.tsx` (Zone 3 Advanced → Continuous Mode), `ContinuousMode.tsx` |
| Modify patient biomarkers | `usePatientBiomarkers.ts`, `PatientPulse.tsx`, `PatientVoiceMonitor.tsx` |
| Modify session cleanup (history) | `commands/archive.rs`, `HistoryWindow.tsx`, `components/cleanup/` (CleanupActionBar, DeleteConfirmDialog, EditNameDialog, MergeConfirmDialog, SplitView), `SplitWindow.tsx` (standalone split window) |
| Modify shadow mode | `shadow_log.rs`, `continuous_mode.rs` (shadow observer task), `config.rs` (`shadow_active_method`, `shadow_csv_log_enabled`) |
| Modify screen capture / vision | `screenshot.rs` (capture, permission check, blank detection), `patient_name_tracker.rs` (name + DOB extraction via `parse_vision_response()`), `screenshot_task.rs` (capture task), `continuous_mode.rs` (integration), `commands/screenshot.rs` |
| Modify replay logging | `segment_log.rs` (per-segment JSONL), `replay_bundle.rs` (encounter test case), `day_log.rs` (day-level events), `continuous_mode.rs` (integration points), `config.rs` (`replay_snapshot()`) |
| Add session-scoped state | `useSessionLifecycle.ts` (add reset call to `resetAllSessionState`) |
| Modify physician/room management | `commands/physicians.rs`, `usePhysicianProfiles.ts`, `PhysicianSelect.tsx`, `AdminPanel.tsx` |
| Modify room setup | `room_config.rs`, `commands/physicians.rs`, `useRoomConfig.ts`, `RoomSetup.tsx` |
| Modify server session sync | `profile_client.rs`, `continuous_mode.rs` (ServerSyncContext), `commands/archive.rs` (server fallback) |
| Modify patient handout | `llm_client.rs` (`build_patient_handout_prompt()`), `commands/ollama.rs` (`generate_patient_handout`), `commands/archive.rs` (`save_patient_handout`, `get_patient_handout`), `local_archive.rs`, `usePatientHandout.ts`, `PatientHandoutEditor.tsx` |
| Modify billing | `commands/billing.rs` (BillingContext toggles), `billing/rule_engine.rs`, `billing/ohip_codes.rs` (234 codes + 21 exclusion groups), `billing/clinical_features.rs` (23 visit types, 79 procedures, 14 conditions), `billing/types.rs`, `billing/time_tracking.rs`, `src/components/billing/BillingTab.tsx`, `src/types/index.ts` (billing types section) |

## IPC Commands (~144 total across 21 modules)

| Module | Commands | Source |
|--------|----------|--------|
| Session (5) | `start_session`, `stop_session`, `reset_session`, `get_audio_file_path`, `reset_silence_timer` | `commands/session.rs` |
| Settings (2) | `get_settings`, `set_settings` | `commands/settings.rs` |
| Audio (1) | `list_input_devices` | `commands/audio.rs` |
| Models (12) | `check_model_status`, `ensure_models`, `download_*_model`, `get_whisper_models`, etc. | `commands/models.rs` |
| LLM/SOAP (6) | `check_ollama_status`, `list_ollama_models`, `prewarm_ollama_model`, `generate_soap_note` (supports `model_override`), `generate_soap_note_auto_detect` (supports `model_override`), `generate_predictive_hint` | `commands/ollama.rs` |
| Medplum (17) | `medplum_*` — auth, patients, encounters, sync, history | `commands/medplum.rs` |
| STT Router (2) | `check_whisper_server_status`, `list_whisper_server_models` | `commands/whisper_server.rs` |
| Permissions (3) | `check_microphone_permission`, `request_*`, `open_*_settings` | `commands/permissions.rs` |
| Listening (3) | `start_listening`, `stop_listening`, `get_listening_status` | `commands/listening.rs` |
| Speaker Profiles (6) | `list_speaker_profiles`, `get_speaker_profile`, `create_*`, `update_*`, `delete_*`, `reenroll_*` | `commands/speaker_profiles.rs` |
| Archive (19) | `get_local_session_dates`, `get_local_sessions_by_date`, `get_local_session_details`, `save_local_soap_note`, `read_local_audio_file`, `delete_local_session`, `split_local_session`, `merge_local_sessions`, `update_session_patient_name`, `renumber_local_encounters`, `get_session_transcript_lines`, `suggest_split_points`, `get_session_feedback`, `save_session_feedback`, `get_session_soap_note`, `delete_patient_from_session`, `rename_patient_label`, `merge_patient_soaps`, `generate_clinical_feedback` | `commands/archive.rs` |
| Clinical Chat (1) | `clinical_chat_send` | `commands/clinical_chat.rs` |
| MIIS (3) | `miis_suggest`, `miis_send_usage`, `generate_ai_image` | `commands/miis.rs`, `commands/images.rs` |
| Screenshot (7) | `check_screen_recording_permission`, `open_screen_recording_settings`, `start/stop_screen_capture`, `get_screen_capture_status`, `get_screenshot_paths`, `get_screenshot_thumbnails` | `commands/screenshot.rs` |
| Continuous (7) | `start/stop_continuous_mode`, `get_continuous_mode_status`, `trigger_new_patient`, `set_continuous_encounter_notes`, `list_serial_ports`, `set_stt_language` | `commands/continuous.rs` |
| Vision (5) | `generate_vision_soap_note`, `run_vision_experiments`, `get_vision_experiment_results`, `get_vision_experiment_report`, `list_vision_experiment_strategies` | `commands/ollama.rs` |
| Physicians (18) | `get_room_config`, `save_room_config`, `test_profile_server`, `get_physicians`, `select_physician`, `get_active_physician`, `deselect_physician`, `sync_speaker_profiles`, `create_physician`, `update_physician`, `delete_physician`, `get_rooms`, `create_room`, `update_room`, `delete_room`, `sync_settings_from_server`, `sync_infrastructure_settings`, `sync_room_settings` | `commands/physicians.rs` |
| Calibration (4) | `start_co2_calibration`, `stop_co2_calibration`, `advance_calibration_phase`, `get_calibration_status` | `commands/calibration.rs` |
| Patient Handout (3) | `generate_patient_handout`, `save_patient_handout`, `get_patient_handout` | `commands/ollama.rs`, `commands/archive.rs` |
| Billing (9) | `get_session_billing`, `save_session_billing`, `confirm_session_billing`, `extract_billing_codes`, `get_daily_billing_summary`, `get_monthly_billing_summary`, `export_billing_csv`, `search_ohip_codes`, `search_diagnostic_codes` | `commands/billing.rs` |

## Events (Backend → Frontend)

| Event | Purpose |
|-------|---------|
| `session_status` | Recording state changes |
| `transcript_update` | Real-time transcript (session mode only) |
| `biomarker_update` | Vitality, stability, session metrics |
| `audio_quality` | Level, SNR, clipping metrics |
| `listening_event` | Auto-detection status (includes `speaker_not_verified`) |
| `silence_warning` | Auto-end countdown (silence_ms, remaining_ms) |
| `session_auto_end` | Session auto-ended due to silence |
| `continuous_mode_event` | Continuous mode status changes (started, encounter_detected, soap_generated, encounter_merged, sensor_status, shadow_decision, retrospective_split, sleep_started, sleep_ended, etc.) |
| `continuous_transcript_preview` | Live transcript preview in continuous mode (separate from `transcript_update`) |
| `calibration_update` | CO2 sensor calibration progress events |
| `deep-link` | OAuth callback URL received via single-instance plugin |

## Session States

```
Idle → Preparing → Recording → Stopping → Completed
  ↑                                           │
  └──────────── Reset / Error ←───────────────┘
```

## Code Patterns & Gotchas

| Pattern | Rule |
|---------|------|
| Concurrent async guards | Use `useRef` (not `useState`) — state is async, can't prevent double-clicks |
| Session lifecycle resets | Add cleanup to `useSessionLifecycle.resetAllSessionState()` |
| Pipeline staleness | Generation counter (`u64`) — discard messages from previous pipeline runs |
| Tauri `listen()` cleanup | Use `mounted` flag + call `fn()` immediately if unmounted before resolve |
| Pipeline handle cleanup | `PipelineHandle` has `Drop` impl; spawn background thread for `h.join()` to avoid blocking Tauri thread |
| Config safety | Clamp loaded values to safe ranges (user could edit JSON manually) |
| Date arithmetic | Use `checked_add_signed` not `+` for chrono dates (prevents panic) |
| UTF-8 slicing | Use `ceil_char_boundary()` for safe substring truncation |
| Serde casing | All frontend-facing structs need `#[serde(rename_all = "camelCase")]` |
| Event namespacing | Session events (`transcript_update`) vs continuous events (`continuous_transcript_preview`) — never share |
| File read commands | Always validate paths are within expected directories (path traversal prevention) |
| Emit after success | Don't emit "started" events before the operation actually succeeds |
| Functional setState | Use `prev => !prev` pattern in `useCallback` to avoid stale closures |
| Vec batch cleanup | Use `drain(..excess)` not `remove(0)` in loop |
| Path traversal prevention | `validate_session_id()` in local_archive.rs, `validate_fhir_id()` in medplum.rs — reject `..` and non-alphanumeric IDs |
| Error body truncation | `truncate_error_body()` in llm_client.rs and medplum.rs — caps HTTP error bodies at 200 chars, uses `ceil_char_boundary()` for UTF-8 safety. Prevents PHI leakage through proxy error pages that echo request bodies |
| Token refresh locking | `get_valid_token()` in medplum.rs — double-check locking pattern to avoid concurrent refresh races |
| Settings validation after update | `clamp_values()` called after `update_from_settings()` in config.rs — safety net for user-edited JSON |
| Encounter notes: clone before clear | In continuous mode detector, clone accumulated notes before clearing buffer to avoid data loss |
| Audio quality shared util | `getAudioQualityLevel()` in utils.ts — shared across RecordingMode, ReviewMode, ContinuousMode |
| Force-split constants | Named constants in encounter_detection.rs: `FORCE_CHECK_WORD_THRESHOLD` (3K), `FORCE_SPLIT_WORD_THRESHOLD` (5K), `FORCE_SPLIT_CONSECUTIVE_LIMIT` (3), `ABSOLUTE_WORD_CAP` (25K). Graduated force-split only counts consecutive LLM failures/timeouts (not confident "no split" responses). Both FORCE_CHECK and FORCE_SPLIT use `cleaned_word_count` (hallucination-stripped) to avoid STT phrase loops inflating past thresholds. Retrospective: `MULTI_PATIENT_CHECK_WORD_THRESHOLD` (2500), `MULTI_PATIENT_SPLIT_MIN_WORDS` (500). Clinical content check: `MIN_WORDS_FOR_CLINICAL_CHECK` (100) — transcripts below this threshold skip the LLM clinical check |
| Presence sensor auto-detect | `auto_detect_port()` in presence_sensor/sources/serial.rs scans USB-serial devices when configured port fails |
| Screen recording permission | Use `CGPreflightScreenCaptureAccess()` (not 1x1 pixel capture) — old check always passed even without permission. `is_blank_capture()` heuristic detects blanked-out window content |
| SOAP JSON repair | Pipeline: `fix_json_newlines` → `remove_leading_commas` → `remove_trailing_commas` → `fix_truncated_json` (closes unclosed strings + missing brackets) → filter empty strings. Raw-JSON fallback returns structured placeholder instead of broken JSON |
| Hallucination filter | Two-phase: single-word repetitions then n-gram phrase loops (sizes 3-25). `strip_hallucinations()` in encounter_experiment.rs |
| SOAP suppression | Non-clinical encounters (`likely_non_clinical=true`) skip SOAP generation entirely. Transcript still archived |
| Confidence gate | Dynamic base threshold: 0.85 for encounters <20 min, 0.7 for 20+ min. `merge_back_count` escalation: each merge-back adds +0.05 (capped at 0.99), reset when split sticks. Prevents repeated false splits on long sessions |
| SOAP retry | `parse_soap_with_retry()` in llm_client.rs: if SOAP parse returns malformed placeholder (`MALFORMED_SOAP_SENTINEL`), retries the LLM call once before giving up |
| Orphaned SOAP recovery | On continuous mode stop, scans today's sessions for `has_soap_note == false` and regenerates SOAP. Skips non-clinical encounters. Uses existing flush LLM client |
| Sensor prompt design | sensor_departed: V2_soft framing lists common false departures, directs LLM to evaluate transcript content. sensor_present: conservative "NOT transitions" framing, proven in production |
| Retrospective multi-patient check | After merge-back, if merged transcript >= 2500 words: (1) `MULTI_PATIENT_CHECK_PROMPT` detects multiple patients (distinguishes companions from separate visits), (2) `MULTI_PATIENT_SPLIT_PROMPT` finds boundary via name transitions, (3) size gate requires both halves >= 500 words. Auto-splits and regenerates SOAP for both halves. Constants in `encounter_detection.rs`, logic in `continuous_mode.rs` |
| Replay logging architecture | Three tiers: `SegmentLogger` (per-segment JSONL, buffers before session dir exists, holds open file handle), `ReplayBundleBuilder` (accumulator pattern, `build_and_reset()` uses `std::mem::take()` for zero-copy write+reset), `DayLogger` (day-level JSONL, immediate writes). All created in continuous mode setup, wired at 12+ integration points. Schema version field for forward compatibility |
| DetectionCheck construction | Use `DetectionCheck::new()` constructor for common fields (sensor context, prompts, loop state), then set result-specific fields (`success`, `response_raw`, `parsed_*`, `error`). Avoids 4x copy-paste at LLM call outcomes |
| Replay bundle lifecycle | Created with `config.replay_snapshot()` at continuous mode start → `add_segment/detection_check/vision_result/sensor_transition` during encounter → `set_split_decision/clinical_check/merge_check/soap_result/name_tracker/outcome` at encounter end → `build_and_reset()` writes JSON and clears for next encounter |
| Config replay snapshot | `config.replay_snapshot()` returns `serde_json::Value` with 20 pipeline-relevant fields (detection mode/model/timing, merge, sensor, SOAP, screen capture). Logged in both `day_log.jsonl` config event and `replay_bundle.json` |
| Server sync fire-and-forget | `ServerSyncContext` in `continuous_mode.rs` — clones IDs+client, spawns async upload task. 30s delayed re-sync catches late-written aux files |
| Hybrid history merge | `commands/archive.rs` — local sessions + server sessions merged by session_id (local wins), server fills gaps for cross-machine sessions |
| Profile cache fallback | `physician_cache.rs` — server fetch with local JSON cache fallback. Cache updated on every successful server fetch |
| Profile server failover | `ProfileClient` stores multiple base URLs (`base_urls: Vec<String>`) with `AtomicUsize` active index. `select_best_url()` probes `/health` on each URL (2s timeout). `connect_timeout(3s)` on main client for fast connection failure detection. `save_room_config` preserves `fallback_server_urls` from existing config |
| Gemini retry | `gemini_client.rs` — single retry with 2s backoff on network errors (DNS, connection, TLS). HTTP errors (4xx/5xx) are not retried |
| Sensor-continuity gate | `sensor_continuous_present` in continuous_mode.rs — tracks unbroken sensor presence since last split. When true, `evaluate_detection()` raises LLM-only split threshold to 0.99. Prevents false splits during couples/family visits where sensor confirms room is occupied |
| Recent encounters staleness | After encounter merge, `recent_encounters` list is updated via `retain()` to remove the merged-away session ID. Prevents click-to-copy from referencing deleted session directories |
| Day log midnight rotation | `DayLogger` stores current date, checks on each `log()` call. Date change → close old file, open new under correct date dir. Same pattern as `csv_logger.rs` |
| Sleep mode clean stop | Sleep scheduler sets inner handle's `stop_flag`, causing `run_continuous_mode` to proceed through normal cleanup. Outer loop detects sleep-triggered stop (vs user stop) and enters sleep wait. DST-safe via `chrono-tz::America::New_York` with `match chrono::LocalResult` for spring-forward gap handling |
| Screenshot word-count gate | `screenshot_task.rs` checks `transcript_buffer.word_count()` at top of capture loop. Empty buffer → skip capture (no speech = empty room, no need for vision calls) |
| Settings merge helper | `buildMergedSettings()` in useSettings.ts — single source of truth for converting `PendingSettings` → full `Settings`. Used by both `saveSettings` and `toggleSetting` in App.tsx |
| SOAP timeout constant | `SOAP_GENERATION_TIMEOUT_SECS` (300) and `SOAP_TIMEOUT_ERROR` ("timeout_300s") in encounter_pipeline.rs — avoids magic numbers and string allocation in timeout path |
| Sensor receiver safety | In continuous_mode.rs detector loop, `hybrid_sensor_rx.as_mut()` uses `let Some(...) else { degrade }` pattern instead of `unwrap()` — graceful fallback to timer-only detection |
| Stop flag contract | `ContinuousModeHandle` has two stop flags: `stop_flag` (inner, cleared by sleep restart) and `user_stop_flag` (outer, never cleared). Sleep MUST only set `stop_flag`; full stop MUST use `handle.stop()` which sets both. Documented on the struct fields |
| Profile service atomic writes | `atomic_write()` in profile-service sessions.rs — writes to UUID-suffixed temp file then renames. Used for transcript.txt, soap_note.txt, metadata.json. Prevents truncated files from mid-write crashes |
| Profile service session cache | `SessionStore.session_cache` — in-memory `HashMap<(physician_id, session_id), PathBuf>` avoids O(N) directory walks. Populated on lookup, invalidated on delete/split/merge |
| Profile service input validation | `validate()` methods on Create/UpdatePhysicianRequest and Create/UpdateRoomRequest — caps name at 500 chars, instructions at 10K, etc. |
| Archive date parsing | `parse_archive_date()` in commands/archive.rs — shared helper for "YYYY-MM-DD" → `DateTime<Utc>` conversion. Used by save_local_soap_note, save_patient_handout, get_patient_handout |
| Patient handout save-then-load | `usePatientHandout` saves handout to archive first, then opens editor window which loads from backend on mount. Avoids event delivery race condition between window creation and React mount |
| Patient handout in SOAP context | `generate_soap_note_auto_detect` checks for `patient_handout.txt` via `get_patient_handout_by_id()` and includes it in the LLM prompt. Uses local date for path construction |
| Billing extraction fail-open | Billing extraction errors in continuous mode are logged but never block encounter processing. `extract_and_archive_billing()` in encounter_pipeline.rs returns `Err`, caller logs warning and continues |
| Billing invalidation on SOAP change | `add_soap_note()` in local_archive.rs auto-deletes billing.json and clears `has_billing_record` when SOAP is regenerated. Same pattern in split_session and merge_encounters |
| Two-stage billing (no LLM hallucination) | LLM extracts clinical features (constrained enums in `clinical_features.rs`), rule engine maps to OHIP codes deterministically (`rule_engine.rs`). LLM never outputs billing codes |
| Vision DOB extraction | `parse_vision_response()` in `patient_name_tracker.rs` — tries JSON `{"name": "...", "dob": "YYYY-MM-DD"}` first, falls back to plain-text parsing. DOB validated as YYYY-MM-DD format. `PatientNameTracker` stores DOB separately via `set_dob()`, cleared on `reset()`. `patient_dob` field in `ArchiveMetadata` auto-populates billing age bracket |
| Billing context toggles | `BillingContext` struct in `commands/billing.rs` — physician-provided context (visit_setting, patient_age, referral_received, counselling_exhausted, after_hours_override). `build_context_hints()` converts to LLM prompt hints. Passed as optional parameter to `extract_billing_codes` |
| OHIP code search + conflicts | `search_ohip_codes()` searches 234 codes by code prefix or description substring. `find_conflicts()` / `find_all_conflicts()` in `ohip_codes.rs` check 21 exclusion groups for code incompatibilities |
| SOAP model override | `model_override: Option<String>` parameter on `generate_soap_note` and `generate_soap_note_auto_detect`. History regenerate button has dropdown for `soap-alt` and `soap-alt-2` aliases |

## Features

| Feature | Summary | Detail |
|---------|---------|--------|
| **SOAP Generation** | Multi-patient auto-detect, defaults to `soap-model-fast` alias (regenerate dropdown supports `soap-alt`, `soap-alt-2` via `model_override`), auto-copy to clipboard, problem-based or comprehensive format, detail level 1-10. Malformed output triggers one LLM retry via `parse_soap_with_retry()`. Orphaned SOAP recovery on continuous mode stop. If a patient handout exists, it is included in the SOAP prompt as context | `llm_client.rs`, ADR 0009/0012 |
| **Patient Handout** | Mid-session patient-friendly visit summary. "Patient Handout" button in RecordingMode and ContinuousMode generates plain-language handout (5th-8th grade reading level) via `soap-model-fast`. Opens standalone editor window (Save/Print/Copy/Close). Saved as `patient_handout.txt` in session archive. Included in SOAP generation context. Viewable in History window "Handout" tab. Does not alter encounter detection | `llm_client.rs`, `commands/ollama.rs`, `commands/archive.rs`, `PatientHandoutEditor.tsx`, `usePatientHandout.ts` |
| **Transcription** | STT Router WebSocket streaming via aliases (`stt_alias`), all 3 modes use streaming, audio preprocessing (DC removal, 80Hz HPF, AGC) | `whisper_server.rs`, ADR 0020 |
| **Auto-Session Detection** | VAD → optimistic recording → parallel greeting check → confirm/discard. Optional speaker verification (`auto_start_require_enrolled`) | `listening.rs`, ADR 0011/0016 |
| **Medplum EMR** | OAuth 2.0 + PKCE via `fabricscribe://` deep link, auto-sync transcript + audio on complete, SOAP auto-added to encounter | `medplum.rs`, ADR 0008 |
| **Biomarkers** | Vitality (F0), Stability (CPP), Cough Detection (YAMNet ONNX), Conversation Dynamics. Thresholds in `types/index.ts` | `biomarkers/`, ADR 0007 |
| **Speaker Enrollment** | 256-dim ECAPA-TDNN embeddings, threshold 0.6 enrolled / 0.3 auto-cluster, context injected into SOAP prompts | `speaker_profiles.rs`, ADR 0014 |
| **Clinical Chat** | LLM chat during recording via `clinical-assistant` alias. Router must handle tool execution server-side | `commands/clinical_chat.rs`, ADR 0017 |
| **Auto-End Silence** | VAD silence → `SilenceWarning` countdown → auto-stop. Config: `auto_end_silence_ms` (default 180s). User can cancel via `reset_silence_timer` | `pipeline.rs`, ADR 0015 |
| **MCP Server** | Port 7101, JSON-RPC 2.0. Tools: `agent_identity`, `health_check`, `get_status`, `get_logs` | `mcp/` |
| **MIIS Images** | LLM extracts concepts every 30s → MIIS returns ranked images. Backend proxies through Rust (CORS). Server needs embedder enabled | `commands/miis.rs`, ADR 0018 |
| **AI Images** | User-driven medical illustration generation via Gemini API. Clinician types a description (anatomy, condition, imaging report) → prompt prefixed with clinical framing → Gemini generates PNG. Image viewer window with Save/Print. Image history window shows all generated images for the session (ephemeral, clears on restart). No cooldown or session cap. Config: `image_source=ai` (default), `gemini_api_key` | `gemini_client.rs`, `commands/images.rs`, `useAiImages.ts`, `ImageSuggestions.tsx`, `ImageViewerWindow.tsx`, `ImageHistoryWindow.tsx` |
| **Differential Diagnosis** | Top 3 DDx shown below Patient Illustration section. Updated every 30s (piggybacks on predictive hint LLM call — no extra API calls). Color-coded likelihood badges (Likely/Possible/Less likely). Hover shows cardinal symptoms/findings via tooltip. Clears on encounter end | `commands/ollama.rs` (DifferentialDiagnosis struct + prompt), `usePredictiveHint.ts`, `ContinuousMode.tsx`, `RecordingMode.tsx` |
| **Continuous Mode** | All-day recording, LLM or sensor-based encounter detection, auto-SOAP per encounter. Sleep mode auto-pauses 10 PM–6 AM EST (clean stop + auto-restart, configurable). Vision-based patient name + DOB extraction via `vision-model` alias (JSON format: `{"name": "...", "dob": "YYYY-MM-DD"}`). `PatientNameTracker` recency-weighted voting (later screenshots count more). DOB auto-populates patient age bracket in billing context. `patient_dob` stored in `ArchiveMetadata`. Screenshot capture gated on transcript word count (skips when buffer empty). Recent encounters list with click-to-copy SOAP; merged sessions auto-removed from list. Retrospective multi-patient check auto-splits incorrectly merged encounters (couples, family visits) | `continuous_mode.rs`, `encounter_detection.rs`, `commands/continuous.rs`, ADR 0019 |
| **Presence Sensor** | Two sensor hardware options: (1) ESP32 Multi-Sensor Bridge (WiFi HTTP): mmWave (SEN0395 24GHz, UART), CO2/temp/humidity (SCD41, I2C), thermal camera (MLX90640 32x24, I2C) at `presence_sensor_url`. (2) XIAO ESP32-C3 (USB serial): 24GHz mmWave (Seeed mmWave for XIAO), 115200 baud, JSON output, no WiFi. Module directory with `SensorSource` trait, `DebounceFsm`, thermal analysis, CO2 tracker, and fusion engine. Fusion currently mmWave-only passthrough; thermal + CO2 tracked for health/monitoring but don't influence presence decision (deferred to per-room calibration). Debounced presence → absence threshold → encounter split. Sensor failure emits SensorStatus (not Error) — continuous mode stays active. Config: `thermal_hot_pixel_threshold_c` (28°C), `co2_baseline_ppm` (420). Firmware: `~/projects/room6-sensor/` (PlatformIO) or `room6-xiao-sensor/` (Arduino) | `presence_sensor/` |
| **Hybrid Detection** | Sensor early-warning + LLM confirmation. Sensor Present→Absent accelerates LLM check (~30s vs ~8 min). Sensor timeout force-splits after `hybrid_confirm_window_secs` (default 180s). Sensor-continuity gate: when sensor shows unbroken presence since last split, LLM-only split confidence threshold raised to 0.99 (prevents false splits during couples/family visits). Sensor-departed prompt (V2_soft) lists common false departures. Graceful LLM-only fallback when sensor unavailable. Handles back-to-back encounters via regular LLM timer. Config: `encounter_detection_mode="hybrid"` | `continuous_mode.rs`, `encounter_detection.rs`, `config.rs` |
| **Shadow Mode** | Dual detection comparison — runs sensor and LLM concurrently, logs decisions to CSV for accuracy analysis. Config: `encounter_detection_mode="shadow"`, `shadow_active_method` | `shadow_log.rs`, `continuous_mode.rs` |
| **Session Cleanup** | History window tools: delete, split, merge sessions, rename patients, renumber encounters. Split opens in separate resizable window with LLM-suggested split point (`suggest_split_points` via `fast-model`) | `commands/archive.rs`, `components/cleanup/`, `SplitWindow.tsx` |
| **Vision Experiments** | CLI + IPC tools for comparing vision-based SOAP strategies across archived sessions | `vision_experiment.rs`, `commands/ollama.rs` |
| **Simulation Replay Logging** | Three-tier structured logging for offline replay and regression testing: per-segment JSONL timeline (`segments.jsonl`), self-contained encounter test case (`replay_bundle.json` — all LLM prompts/responses, sensor transitions, vision results, split decisions), day-level orchestration events (`day_log.jsonl`). Config snapshot via `replay_snapshot()`. ~0.5-3MB/day. `detection_replay_cli` replays archived decisions through `evaluate_detection()` with `--override` for what-if parameter tuning | `segment_log.rs`, `replay_bundle.rs`, `day_log.rs`, `config.rs`, `tools/detection_replay_cli.rs` |
| **FHO+ Billing** | Two-stage billing extraction: LLM extracts clinical features (23 visit types, 79 procedures, 14 conditions), deterministic rule engine maps to OHIP codes. 234 OHIP codes (SOB-verified Apr 2026, including epidurals/nerve blocks from audit), 562 diagnostic codes (MOH ICD-8), 21 exclusion groups. Companion code auto-add: E542A tray fee, E430A pap tray, E079A smoking cessation. Base+add-on quantity logic (G370→G371, G384→G385). Billing preferences in Settings (visit setting, K013 exhausted, hospital-based). Diagnostic code search + LLM suggestion from SOAP. Billing context toggles: visit setting, patient age (auto from vision DOB), referral, K013, after-hours, hospital. CSV export with diagnostic code column. Auto-extracts after SOAP in continuous mode. Full Q310-Q313 time tracking with 14hr/day and 240hr/28-day caps. Daily/monthly summary. Stored as `billing.json` per session | `billing/`, `commands/billing.rs`, `encounter_pipeline.rs`, `src/components/billing/` |
| **Multi-User** | Room + physician profile system. Passwordless physician selection (physical clinic security). Profile service on Mac Studio (:8090) stores physicians, rooms, speakers, and sessions. Server is source of truth — local archive is write-through cache. Settings merge: infrastructure (shared) → room (per-machine) → physician (roaming). Background audio upload, 30s delayed re-sync for late-written files. Offline resilience with cached profiles. Multi-URL failover: `fallback_server_urls` in room_config.json, startup health probe selects fastest responding URL (2s timeout per URL, `connect_timeout` 3s on main client) | `profile_client.rs`, `room_config.rs`, `physician_cache.rs`, `commands/physicians.rs` |

### Continuous Mode Lifecycle Notes
- `started` event emitted only after pipeline successfully starts
- `isActive=false` on `error` events (prevents stale UI state)
- Listening mode disabled while continuous mode is active
- Charting mode switch to "session" blocked while continuous recording is active
- Transcript preview uses `continuous_transcript_preview` event (separate namespace from session)
- Flush-on-stop: when continuous mode stops with buffered transcript (>100 words), the flush path now mirrors the normal encounter split pipeline — metadata enrichment (`charting_mode`, `encounter_number`, `detection_method="flush"`, `patient_name`), clinical content check (non-clinical transcripts skip SOAP), merge check (runs before SOAP to avoid wasted LLM calls), accurate `encounter_started_at` from `TranscriptBuffer.first_timestamp()`. Fail-open: LLM errors during clinical check → assume clinical
- Shared pipeline helpers in `encounter_pipeline.rs`: SOAP generation (`generate_and_archive_soap()`), merge checks (`run_merge_check()`), clinical content checks, metadata enrichment — used by both the main detector loop and flush-on-stop path. Eliminates duplication across 8 call sites
- Detection decisions are a single source of truth via `evaluate_detection()` pure function in `encounter_detection.rs` — called from production loop and replayable offline via `detection_replay_cli`
- Screenshot task logic extracted to `screenshot_task.rs` — periodic capture, blank detection, vision name extraction, stale vote suppression. Word-count gated: skips capture when transcript buffer is empty (no speech = no need for vision)
- Sleep mode: outer loop in `commands/continuous.rs` wraps `run_continuous_mode`. At `sleep_start_hour` EST (default 22), stops pipeline cleanly. During sleep window, UI shows sleep banner. At `sleep_end_hour` EST (default 6), auto-starts fresh continuous mode run. Uses `chrono-tz::America::New_York` for DST-safe EST/EDT handling. User can stop during sleep (30s check interval)
- Recent encounters: `recent_encounters` list (max 3) tracks last split sessions with click-to-copy SOAP. Merged sessions are removed from the list after merge to prevent stale session ID references
- Day log midnight rotation: `DayLogger` checks date on each `log()` call; if `Local::now()` date differs from stored date, closes current file and opens new one under the correct date directory
- Sensor-continuity gate: `sensor_continuous_present` bool tracks whether sensor has shown unbroken presence since last split. Set `true` after successful split when sensor is present; cleared on Present→Absent transition. When true, LLM-only splits require confidence ≥0.99 via `DetectionEvalContext`

## Settings Schema

Source of truth: `src-tauri/src/config.rs` (Rust) / `src/types/index.ts` (TypeScript).

Key settings groups: STT Router (whisper_server_url, stt_alias=`"medical-streaming"`, stt_postprocess=true, language=`"auto"` (auto-detect, default since Gemma 4 STT migration)), Audio (VAD, diarization, enhancement), LLM Router (soap_model=`"soap-model-fast"`, soap_model_fast=`"soap-model-fast"`, fast_model=`"fast-model"`), Medplum (OAuth, auto_sync), Auto-detection (auto_start, auto_end_silence_ms=180000), SOAP (detail_level 1-10, format, custom_instructions), Images (image_source=`"ai"` (default)|`"miis"`|`"off"`, gemini_api_key), MIIS, Screen Capture, Continuous Mode (charting_mode, encounter_check_interval_secs=120, encounter_silence_trigger_secs=45, encounter_merge_enabled, encounter_detection_model=`"fast-model"`, encounter_detection_nothink=false), Sleep Mode (sleep_mode_enabled=true, sleep_start_hour=22, sleep_end_hour=6 — hours in EST, clamped 0-23), Presence Sensor (encounter_detection_mode=`"hybrid"`, presence_sensor_port, presence_absence_threshold_secs=180, presence_debounce_secs=15, presence_csv_log_enabled=true, thermal_hot_pixel_threshold_c=28.0, co2_baseline_ppm=420.0), Shadow Mode (shadow_active_method=`"sensor"`, shadow_csv_log_enabled=true), Hybrid Detection (hybrid_confirm_window_secs=180, hybrid_min_words_for_sensor_split=500), Screen Capture (screen_capture_enabled, screen_capture_interval_secs=30, requires Screen Recording permission), Debug.

Multi-user: profile_server_url + fallback_server_urls (in room_config.json), active_physician_id.

## File Locations

| Path | Contents |
|------|----------|
| `~/.transcriptionapp/models/` | Whisper, speaker embedding, enhancement, YAMNet models |
| `~/.transcriptionapp/config.json` | Settings |
| `~/.transcriptionapp/speaker_profiles.json` | Enrolled speaker voice profiles |
| `~/.transcriptionapp/medplum_auth.json` | OAuth tokens |
| `~/.transcriptionapp/archive/` | Local session archive (`YYYY/MM/DD/session_id/`) |
| `~/.transcriptionapp/archive/YYYY/MM/DD/day_log.jsonl` | Day-level orchestration events (config snapshot, splits, merges, SOAP results) |
| `~/.transcriptionapp/archive/YYYY/MM/DD/session_id/segments.jsonl` | Per-segment timeline (timestamp, text, speaker, word counts) |
| `~/.transcriptionapp/archive/YYYY/MM/DD/session_id/replay_bundle.json` | Self-contained encounter test case (all LLM calls, decisions, outcomes) |
| `~/.transcriptionapp/archive/YYYY/MM/DD/session_id/patient_handout.txt` | Patient-facing visit summary (optional, only if clinician generated one) |
| `~/.transcriptionapp/archive/YYYY/MM/DD/session_id/billing.json` | Per-session billing record (OHIP codes, time entries, totals, draft/confirmed status) |
| `~/.transcriptionapp/logs/` | Activity logs (daily rotation, PHI-safe) |
| `~/.transcriptionapp/debug/` | Debug storage (dev only) |
| `~/.transcriptionapp/mmwave/` | Presence sensor CSV logs (daily rotation) |
| `~/.transcriptionapp/shadow/` | Shadow mode CSV logs (dual detection comparison) |
| `~/sensor-logs/` | Multi-sensor data logger — JSONL + thermal PNGs. `sensor_logger.py` polls ESP32 every 5s |
| `~/sensor-logs/data/YYYY-MM-DD/sensor_log.jsonl` | All sensor data per poll (presence, CO2, temp, humidity, 768-float thermal frame) |
| `~/sensor-logs/data/YYYY-MM-DD/thermal/*.png` | Thermal camera snapshots (iron colormap, every 30s) |
| `~/.transcriptionapp/room_config.json` | Room config (room name, profile server URL, fallback URLs, room ID) |
| `~/.transcriptionapp/cache/physicians.json` | Cached physician list from server |
| `~/.transcriptionapp/cache/physician_{id}.json` | Cached individual physician settings |
| `docs/OHIP_CODE_UPDATE_GUIDE.md` | OHIP code database update procedure (SOB PDF → extract → generate → verify) |
| `scripts/extract_sob_fees.py` | Extract fee data from Schedule of Benefits PDF |
| `scripts/generate_ohip_codes.py` | Generate `ohip_codes.rs` from extracted fees |
| `scripts/verify_ohip_codes.py` | Verify OHIP code database against source data |
| `scripts/audit_ohip_codes.py` | Comprehensive SOB audit — extracts ALL codes from PDF, cross-refs against DB, finds missing GP-billable codes |
| `scripts/test_billing_extraction.py` | Billing extraction basic integration tests (12 cases) |
| `scripts/test_billing_stress.py` | Billing extraction stress tests (15 cases, 80% pass target) |

## Clinic Deployment

| Machine | Role | IP (Tailscale) | LAN IP | User | Notes |
|---------|------|----------------|--------|------|-------|
| MacBook | Server | 100.119.83.76 | 10.241.15.154 | arash | Runs all backend services |
| iMac | Room 2 workstation | 100.74.186.113 | — | room2 (pw: 1278) | Has Node, Rust, pnpm installed |
| This computer | Room 6 workstation | local | — | backoffice | Primary development machine |

## External Services (on MacBook 100.119.83.76 / 10.241.15.154)

| Service | Port | Purpose |
|---------|------|---------|
| STT Router | 8001 | WebSocket streaming transcription (alias: `medical-streaming`) |
| LLM Router | 8080 | SOAP generation, encounter detection, vision-based patient name extraction (`vision-model` alias) |
| Profile Service | 8090 | Physician profiles, room config, centralized session storage, speaker enrollments |
| Medplum | 8103 | EMR/FHIR |
| MIIS | 7843 | Medical illustration images |
| Gemini | (external) | AI image generation (`gemini-3.1-flash-image-preview`) via `generativelanguage.googleapis.com` |
| ESP32 Sensor | (per-room) | Room presence (mmWave + thermal + CO2). WiFi bridge or USB serial (XIAO ESP32-C3, port `/dev/cu.usbmodem21201` on Room 6). Configured per room in admin panel |

## Frontend Structure

**Mode Components** (`src/components/modes/`):
- `ReadyMode.tsx` - Pre-recording (checklist, device selection)
- `RecordingMode.tsx` - Active recording (timer, quality, transcript preview)
- `ReviewMode.tsx` - Post-recording (transcript, SOAP, EMR sync)
- `ContinuousMode.tsx` - Continuous charting dashboard (monitoring, live transcript, encounter stats)

**Key Components** (`src/components/`):
- `Header.tsx` - App header with mode controls and navigation
- `AuthProvider.tsx` - Medplum OAuth context provider
- `LoginScreen.tsx` - OAuth login flow UI
- `ErrorBoundary.tsx` - React error boundary with fallback UI
- `SettingsDrawer.tsx` - 4-zone settings panel (Clinical Workflow → Connection Status → Advanced [collapsed] → Speaker Profiles [sub-view]). 5 visible controls at first open; IT/developer settings behind Advanced accordion. ~675 lines
- `HistoryView.tsx` / `HistoryWindow.tsx` - Session archive browsing (Gmail-style split-pane: calendar+list left, detail right)
- `Calendar.tsx` - Date picker for archive history
- `PatientSearch.tsx` - Medplum patient search
- `PatientPulse.tsx` - Glanceable biomarker summary (replaces verbose BiomarkersSection)
- `PatientVoiceMonitor.tsx` - Patient-focused voice metric trending
- `AudioPlayer.tsx` - Session audio playback
- `AudioQualitySection.tsx` - Mic level/SNR/clipping display
- `SpeakerEnrollment.tsx` - Speaker voice enrollment UI
- `ClinicalChat.tsx` - Clinical assistant chat panel
- `ImageSuggestions.tsx` - Medical illustration display (AI-generated via Gemini or MIIS server)
- `EncounterBar.tsx` - Active encounter status in continuous mode
- `SyncStatusBar.tsx` - EMR sync status indicator
- `ConversationDynamicsSection.tsx` - Turn-taking and engagement metrics
- `BiomarkersSection.tsx` - Detailed biomarker display (legacy, PatientPulse preferred)
- `ActivePhysicianBadge.tsx` - Shows current physician name in header with switch button
- `PhysicianSelect.tsx` - Full-screen physician selection grid (touch-friendly)
- `RoomSetup.tsx` - First-run room setup: room name, server URL, connectivity test
- `RoomSelect.tsx` - Room selection for existing rooms
- `AdminPanel.tsx` - Tabbed admin panel for physician + room CRUD management
- `CalibrationWindow.tsx` - CO2 sensor calibration window (standalone)
- `FeedbackPanel.tsx` - Session feedback/rating UI
- `ImageViewerWindow.tsx` - Medical illustration viewer with Save/Print toolbar (standalone window)
- `ImageHistoryWindow.tsx` - Session image history grid with thumbnail/detail views (standalone window)
- `PatientHandoutEditor.tsx` - Patient handout editor (standalone window — Save/Print/Copy/Close)

**Billing Components** (`src/components/billing/`):
- `BillingTab.tsx` - Per-encounter billing panel (code list with confidence + quantity columns, context toggles, code search, time entries, totals, confirm). Context toggles: visit setting, patient age, referral, K013, after-hours
- `DailySummaryView.tsx` - Daily billing summary with cap progress bars
- `MonthlySummaryView.tsx` - 28-day rolling summary with FHO+ cap tracking
- `CapProgressBar.tsx` - Reusable cap progress bar with warning colors
- `billingUtils.ts` - Formatting helpers (formatCents, capWarningColor)

**Cleanup Components** (`src/components/cleanup/`):
- `CleanupActionBar.tsx` - Toolbar with delete/split/merge/rename actions for session cleanup
- `DeleteConfirmDialog.tsx` - Confirmation dialog for session deletion
- `EditNameDialog.tsx` - Dialog for renaming patient name on a session
- `MergeConfirmDialog.tsx` - Confirmation dialog for merging adjacent sessions
- `SplitView.tsx` - Transcript line viewer for selecting split points (inline, legacy)

**Split Window** (`src/components/SplitWindow.tsx`):
- Standalone window for splitting sessions (opened from HistoryWindow)
- LLM-suggested split point via `suggest_split_points` command
- Context passed via URL query params, completion via `emitTo`
- Entry: `split.html` → `src/split.tsx` → `<SplitWindow />`

**Key Hooks** (`src/hooks/`):
- `useSessionLifecycle` - Centralized session start/reset coordination across all hooks
- `useSessionState` - Recording state, transcript, biomarkers
- `useSoapNote` - SOAP generation
- `useMedplumSync` - EMR sync with encounter tracking
- `useSettings` - Configuration management (`PendingSettings` = 25 UI-editable fields, subset of full `Settings`; `diarization_enabled`/`whisper_mode` hardcoded in save, not in pending). Exports `buildMergedSettings()` shared helper for PendingSettings→Settings conversion
- `useAutoDetection` - Listening mode
- `useSpeakerProfiles` - Speaker enrollment CRUD operations
- `useClinicalChat` - Clinical assistant chat during recording
- `usePredictiveHint` - LLM hints + concept extraction during recording
- `useMiisImages` - Medical illustration suggestions from MIIS server
- `useAiImages` - AI-generated medical images via Gemini API
- `useContinuousMode` - Continuous charting mode state and controls
- `usePatientBiomarkers` - Patient-focused biomarker trending for continuous mode
- `useOllamaConnection` - LLM router connection status and model listing
- `useConnectionTests` - Pre-flight connectivity tests for STT/LLM/Medplum services
- `useContinuousModeOrchestrator` - Coordinates continuous mode lifecycle across hooks
- `useScreenCapture` - Periodic screenshot capture during recording
- `useChecklist` - Pre-flight system checks
- `useDevices` - Audio input device enumeration
- `useWhisperModels` - Whisper model download and management
- `useRoomConfig` - Room config load/save, first-run detection
- `usePhysicianProfiles` - Physician list fetch, select/deselect, cache status
- `useAdminPanel` - Admin panel CRUD operations for physicians and rooms
- `useAppUpdater` - GitHub Releases auto-update check + install
- `usePatientHandout` - Generate patient handout via LLM, save to archive, open editor window

**Types**: `src/types/index.ts` - All TypeScript interfaces mirroring Rust structs, biomarker thresholds and status helpers

## Common Issues

| Problem | Solution |
|---------|----------|
| "Model not found" | Check `~/.transcriptionapp/models/` for ONNX models (diarization, enhancement) |
| ONNX tests failing | Set `ORT_DYLIB_PATH` environment variable |
| Audio device errors | Check macOS microphone permissions |
| OAuth opens new instance | Use `pnpm tauri build --debug`, not `tauri dev` |
| Deep links not working | Ensure app was built and `fabricscribe://` scheme registered |
| Clinical chat shows raw JSON | Router must execute tools for `clinical-assistant` alias |
| Speaker verification fails | Ensure profiles exist and speaker model at `~/.transcriptionapp/models/speaker_embedding.onnx` (or legacy `voxceleb_ECAPA512_LM.onnx`) |
| Auto-end too aggressive | Increase `auto_end_silence_ms` or disable `auto_end_enabled` |
| SOAP not copying to clipboard | Check Tauri clipboard plugin permissions |
| MIIS images not loading | Check CSP allows MIIS server domain in `tauri.conf.json` |
| MIIS same images for all queries | Server needs embedder enabled for semantic matching |
| Continuous mode not detecting encounters | Check LLM router connection, increase `encounter_check_interval_secs` |
| Continuous mode UI not showing | Verify `charting_mode: "continuous"` in config.json, restart app |
| "Auto-charted" badge not appearing | Session was created in session mode, not continuous mode |
| Can't switch charting mode | Stop continuous recording before switching from continuous to session mode |
| Auto-detection runs during continuous | Verify `isContinuousMode` guard in App.tsx listening effect |
| Presence sensor "Device or resource busy" | Another process (e.g., `mmwave_logger.py`) holds the serial port — kill it or stop it before starting continuous mode |
| Encounter detection not splitting | Check activity logs for `consecutive_llm_failures` count; force-split fires at 5K cleaned words + 3 consecutive LLM failures (errors/timeouts only — confident "no split" responses don't count), absolute cap at 25K cleaned words. FORCE_CHECK (3K cleaned words) triggers check even below timer interval |
| Screenshots blank / vision always NOT_FOUND | Screen Recording permission not granted — macOS blanks other apps' windows. Toggle off/on in System Settings → Privacy & Security → Screen Recording. Rebuilds may invalidate old permission |
| Vision name extraction 0 successes | Check activity logs for "Vision name extraction failed" (connection) vs "Vision did not find a patient name" (blank captures). If all NOT_FOUND, likely screen recording permission issue |
| Profile server unreachable | Check `http://100.119.83.76:8090/health` (Tailscale) or `http://10.241.15.154:8090/health` (LAN). App probes all URLs in `room_config.json` at startup (primary + `fallback_server_urls`) with 2s timeout, selects first responder. Falls back to cached profiles if all URLs fail |
| Physician switch blocked | Stop active recording or continuous mode before switching physicians |
| History shows sessions from other machines | Expected — server returns all sessions for the physician across all rooms |
| Recent encounter click doesn't copy SOAP | Session may have been merged — merged sessions are auto-removed from recent encounters list. If stale entries persist, they reference deleted session IDs |
| Gemini image generation "error sending request" | Transient network error — client retries once with 2s backoff. If persistent, check internet connectivity and API key in config.json |
| Continuous mode doesn't auto-stop at night | Check `sleep_mode_enabled: true` in config.json. Sleep window uses EST (America/New_York timezone). Verify `sleep_start_hour` and `sleep_end_hour` |
| Billing codes wrong or missing | Run `python scripts/verify_ohip_codes.py` to check database against source. See `docs/OHIP_CODE_UPDATE_GUIDE.md` for full update procedure |
| Billing age not auto-populated | Vision DOB extraction requires Screen Recording permission + `vision-model` alias working. Check `patient_dob` in session metadata |

## E2E Integration Tests

End-to-end tests verify the full pipeline against live STT and LLM Router services. They live in `src-tauri/src/e2e_tests.rs` and are marked `#[ignore]` so they don't run during normal `cargo test`.

### Daily Preflight Script

```bash
./scripts/preflight.sh           # Quick check (~10s) — layers 1-3
./scripts/preflight.sh --full    # Full pipeline (~30s) — layers 1-5
./scripts/preflight.sh --layer 2 # Specific layer only
```

### Running Tests Directly

```bash
cd src-tauri

# All E2E tests (run one at a time — concurrent WebSocket streams can overload STT Router)
cargo test e2e_layer1 -- --ignored --nocapture  # STT Router
cargo test e2e_layer2 -- --ignored --nocapture  # LLM Router
cargo test e2e_layer3 -- --ignored --nocapture  # Local Archive
cargo test e2e_layer4 -- --ignored --nocapture  # Session mode full pipeline
cargo test e2e_layer5 -- --ignored --nocapture  # Continuous mode full pipeline

# Single test
cargo test e2e_layer2_hybrid -- --ignored --nocapture
```

### Test Layers

| Layer | What it Tests | Services Required |
|-------|--------------|-------------------|
| 1 | STT Router health, alias, WebSocket streaming | STT Router |
| 2 | SOAP generation, encounter detection (fast-model), hybrid model + merge + hallucination filter | LLM Router |
| 3 | Archive save/retrieve, continuous mode metadata | Filesystem only |
| 4 | Session mode: Audio → STT → SOAP → Archive → History | STT + LLM Router |
| 5 | Continuous mode: Audio → STT → Detection → SOAP → Archive → History | STT + LLM Router |

### Hybrid Model Configuration

E2E tests use the production model configuration:
- **Detection**: `fast-model` (~7B) — default encounter detection model
- **Merge**: `fast-model` (~7B) + patient name (M1 strategy) — better semantic understanding
- **SOAP**: `soap-model-fast` — dedicated SOAP generation model (also `soap-alt`, `soap-alt-2` via model_override)
- **Vision**: `vision-model` (hard-coded, not configurable) — patient name extraction from screenshots
- **Clinical chat**: `clinical-assistant` (hard-coded, not configurable) — router must handle tool execution

Config fields in `config.rs`: `encounter_detection_model` (default "fast-model"), `encounter_detection_nothink` (default false)

### Troubleshooting E2E Failures

| Failure | Likely Cause | Fix |
|---------|-------------|-----|
| Layer 1 health check | STT Router down | Check `http://100.119.83.76:8001/health` |
| Layer 1 streaming "Connection reset" | Too many concurrent WebSocket connections | Run tests one layer at a time |
| Layer 2 SOAP empty | LLM Router down or model not loaded | Check `http://100.119.83.76:8080/health` |
| Layer 2 detection not complete | Model changed or prompt regression | Run encounter experiment CLI to compare |
| Layer 2 merge says different | Patient name not in prompt or model regression | Check `build_encounter_merge_prompt()` |
| Layer 3 archive failure | Disk permissions | Check `~/.transcriptionapp/archive/` writable |
| Layer 4/5 "STT returned 4 chars" | Normal — sine wave test audio produces no speech | Test uses fixture transcript as fallback |

### Experiment CLIs

For deeper investigation of model accuracy and detection replay:

```bash
cd src-tauri

# Encounter detection experiments (replays archived transcripts through different prompts)
cargo run --bin encounter_experiment_cli
cargo run --bin encounter_experiment_cli -- --model fast-model
cargo run --bin encounter_experiment_cli -- --detect-only p0 p3

# Vision SOAP experiments
cargo run --bin vision_experiment_cli

# Detection replay (replays archived detection decisions through evaluate_detection())
cargo run --bin detection_replay_cli -- ~/.transcriptionapp/archive/2026/03/12/
cargo run --bin detection_replay_cli -- --all
cargo run --bin detection_replay_cli -- --all --mismatches
cargo run --bin detection_replay_cli -- --all --override hybrid_confirm_window_secs=120
```

## Testing Best Practices

- Avoid `vi.useFakeTimers()` with React async - conflicts with RTL's `waitFor`
- Use `mockImplementation` with command routing instead of `mockResolvedValueOnce` chains
- Always clean up timers in `beforeEach`/`afterEach`
- Run E2E tests one layer at a time to avoid STT Router WebSocket concurrency limits

## Adding New Features

1. **Config**: Add field to `config.rs`, `types/index.ts`. If UI-visible: also add to `PendingSettings` in `useSettings.ts` + `buildMergedSettings()` + `SettingsDrawer.tsx` (Zone 1 for clinical, Zone 3 Advanced for IT/infra). Config-only settings: edit `~/.transcriptionapp/config.json` directly
2. **Model**: Add URL/download in `commands/models.rs`, check in `checklist.rs`
3. **Command**: Add to `commands/*.rs`, register in `lib.rs`
4. **Pipeline**: Add provider initialization in `pipeline.rs`

## ADRs

See `docs/adr/` for Architecture Decision Records:

| ADR | Title |
|-----|-------|
| 0001 | Use Tauri for desktop app |
| 0002 | Whisper for transcription |
| 0003 | VAD-gated processing |
| 0004 | Ring buffer audio pipeline |
| 0005 | Session state machine |
| 0006 | Speaker diarization |
| 0007 | Biomarker analysis |
| 0008 | Medplum EMR integration |
| 0009 | Ollama SOAP generation |
| 0010 | Audio preprocessing |
| 0011 | Auto-session detection |
| 0012 | Multi-patient SOAP generation |
| 0013 | LLM router migration |
| 0014 | Speaker enrollment system |
| 0015 | Auto-end silence detection |
| 0016 | Speaker-verified auto-start |
| 0017 | Clinical assistant chat |
| 0018 | MIIS medical illustration integration |
| 0019 | Continuous charting mode (end of day) |
| 0020 | STT Router streaming integration |
