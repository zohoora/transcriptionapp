# Profile Service

Standalone axum REST API for multi-user physician profile management, centralized session storage, speaker enrollment, and mobile job tracking. Runs on the MacBook server (100.119.83.76:8090) within the AMI Assist clinic deployment.

## Quick Start

```bash
cargo check                  # Type check
cargo test                   # Run tests
cargo run                    # Start server (default port 8090)
cargo run -- --port 9090     # Custom port
```

## Architecture

```
src/
├── main.rs            # CLI args, startup, auth key wiring
├── lib.rs             # create_app_state() + build_app(), middleware stack
├── auth.rs            # Optional API key middleware (PROFILE_API_KEY env var)
├── error.rs           # ApiError enum → HTTP status mapping
├── types.rs           # All data structures + validation methods
├── routes/
│   ├── mod.rs         # build_router() — 32 route registrations across 8 resource groups
│   ├── health.rs      # GET /health
│   ├── infrastructure.rs  # Clinic-wide settings (singleton)
│   ├── config_data.rs # GET/PUT for prompts, billing, thresholds + version check (9 handlers)
│   ├── mobile.rs      # Mobile job upload, status, CRUD (6 handlers)
│   ├── physicians.rs  # Physician CRUD
│   ├── rooms.rs       # Room CRUD
│   ├── sessions.rs    # Session storage, split, merge, audio, files, day-log
│   └── speakers.rs    # Speaker profile CRUD
└── store/
    ├── mod.rs         # AppState (6 RwLock<Manager> + SessionStore)
    ├── config_data.rs # ConfigDataStore — prompts, billing rules, thresholds, operational defaults; version counter
    ├── mobile_jobs.rs # Mobile job store (in-memory HashMap + JSON persistence)
    ├── physicians.rs  # In-memory Vec + atomic JSON persistence
    ├── rooms.rs       # Same pattern as physicians
    ├── speakers.rs    # Same pattern
    ├── infrastructure.rs  # Singleton JSON
    └── sessions.rs    # Disk-based session store + in-memory path cache
```

Middleware stack (outermost → innermost): CORS → body limit (500 MB) → auth → handler.

## Key Patterns

| Pattern | Rule |
|---------|------|
| Atomic writes | `atomic_write()` — UUID-suffixed temp file + rename. Used for transcript, SOAP, metadata, all JSON stores |
| Session cache | `session_cache: HashMap<(physician_id, session_id), PathBuf>` — avoids O(N) directory walk per lookup. Populated lazily, invalidated on delete/split/merge |
| Path traversal | `validate_id()` rejects `/`, `\`, `..`, `\0`, empty strings. Called on all physician_id and session_id inputs |
| File allowlist | `is_allowed_session_file()` in `store/sessions.rs` — explicit allowlist for auxiliary files. Currently allows: `pipeline_log.jsonl`, `replay_bundle.json`, `segments.jsonl`, `billing.json`, `patient_handout.txt`, and `screenshots/*.jpg`. **Note**: `feedback.json` and `patient_labels.json` are not in the allowlist — they have dedicated typed routes. Adding new aux file types requires updating both the allowlist and the tauri-side `server_sync.rs::SYNCED_AUX_FILES` |
| Metadata patch (JSON merge) | `patch_metadata()` accepts `serde_json::Value` (raw JSON object) and merges non-null fields into existing metadata. Replaced the v0.10.30 typed-struct approach so new tauri-side metadata fields propagate without requiring a profile-service rebuild. Tradeoff: no compile-time type checking on patch keys — bad keys are silently merged. The store re-serializes after merge so unknown fields persist for future tauri reads |
| Input validation | `validate()` methods on all Create/Update request types — name 500 chars, instructions 10K, etc. |
| Backup safety | Split/merge operations create `.bak` files before modifying transcripts, removed on success |
| JSON stores | Physicians, rooms, speakers, infrastructure — in-memory Vec + `atomic_write` to disk on mutation. `0o600` file permissions |
| Mobile job idempotency | `recording_index: HashMap<String, String>` — O(1) lookup by `recording_id` to prevent duplicate job creation on upload retry |
| Mobile job persistence | In-memory `HashMap<String, MobileJob>` + atomic JSON write on every mutation. Audio files stored as `{job_id}.m4a` in `mobile_uploads/` |
| axum path params | axum 0.7 uses `:id` syntax (NOT `{id}`) |

## API Routes

32 `.route()` registrations (each with multiple HTTP methods, ~52 handler functions total) across 8 resource groups. Session routes scoped under `/physicians/:id/sessions/...`. Mobile routes under `/mobile/...`. Config routes under `/config/...`.

| Resource | Endpoints |
|----------|-----------|
| Health | `GET /health` |
| Infrastructure | `GET/PUT /infrastructure` |
| Config Data | `GET /config/version`, `GET/PUT /config/prompts`, `GET/PUT /config/billing`, `GET/PUT /config/thresholds`, `GET/PUT /config/defaults` |
| Mobile Jobs | `POST /mobile/upload`, `GET /mobile/jobs`, `GET/PUT/DELETE /mobile/jobs/:job_id`, `GET /mobile/uploads/:job_id` |
| Physicians | `GET/POST /physicians`, `GET/PUT/DELETE /physicians/:id` |
| Rooms | `GET/POST /rooms`, `GET/PUT/DELETE /rooms/:id` |
| Speakers | `GET/POST /speakers`, `GET/PUT/DELETE /speakers/:id` |
| Sessions | dates, list, get, upload, delete, split, merge, renumber, metadata, soap, feedback, patient-name, transcript-lines, audio, files, screenshots, day-log |

## Server Config Categories

Four independent config sections share `config_version.json` (single shared counter, bumped on any update):
- **PromptTemplates** (`prompt_templates.json`) — LLM prompt overrides
- **BillingData** (`billing_data.json`) — OHIP codes + rule tables
- **DetectionThresholds** (`detection_thresholds.json`) — algorithm internals: word thresholds (force-check/split, absolute cap, multi-patient), confidence gates, timeouts, Cat A extensions (`multi_patient_detect_word_threshold`, `vision_skip_streak_k`, `vision_skip_call_cap`, `vision_re_sample_interval_secs`, `gemini_generation_timeout_secs`, `detection_prompt_max_words`)
- **OperationalDefaults** (`operational_defaults.json`) — admin-facing knobs: sleep hours (start/end), thermal/CO2 baselines, encounter intervals (check/silence), 4 model aliases (soap_model, soap_model_fast, fast_model, encounter_detection_model). `validate()` enforces bounds (sleep hours 0-23 and must differ; thermal 20-40°C; CO2 300-600 ppm; check interval 10-3600s; silence trigger 5-600s; non-empty model aliases) — invalid PUT returns 400.

## Data Storage

File-based, under `~/.fabricscribe/` (configurable via `--data-dir`):

```
profile-data/
├── physicians.json          # All physician profiles
├── rooms.json               # All room configs
├── speakers.json            # Speaker voice profiles
├── infrastructure.json      # Clinic-wide settings
├── prompt_templates.json    # Server-configurable LLM prompt templates
├── billing_data.json        # Server-configurable billing rules (OHIP codes, mappings, etc.)
├── detection_thresholds.json # Server-configurable detection thresholds
├── operational_defaults.json # Server-configurable operational knobs (sleep hours, sensor baselines, model aliases, encounter timing)
├── config_version.json      # Shared config version counter (bumped on any config change)
├── mobile_jobs.json         # Mobile recording job queue + status
├── mobile_uploads/          # Uploaded mobile audio files ({job_id}.m4a)
└── sessions/
    └── {physician_id}/
        └── {YYYY}/{MM}/{DD}/
            └── {session_id}/
                ├── metadata.json
                ├── transcript.txt
                ├── soap_note.txt
                ├── audio.wav
                ├── feedback.json
                ├── patient_labels.json
                ├── pipeline_log.jsonl
                ├── replay_bundle.json
                ├── segments.jsonl
                ├── billing.json
                ├── patient_handout.txt
                └── screenshots/*.jpg
```

## Common Tasks

| Task | Files |
|------|-------|
| Add physician field | `types.rs` (struct + Create/UpdateRequest + validate), `store/physicians.rs` (merge logic) |
| Add room setting | `types.rs` (RoomOverlay + Create/UpdateRoomRequest + validate), `store/rooms.rs` |
| Add session endpoint | `routes/sessions.rs` (handler), `store/sessions.rs` (logic), `routes/mod.rs` (register) |
| Add mobile job field | `store/mobile_jobs.rs` (MobileJob struct + UpdateJobRequest), `routes/mobile.rs` (handlers) |
| Modify file allowlist | `store/sessions.rs` (`is_allowed_session_file()`) — currently allows: pipeline_log, replay_bundle, segments, screenshots/*.jpg, billing.json, patient_handout.txt. Update tauri-side `SYNCED_AUX_FILES` in `server_sync.rs` to match |
| Add/update prompt template | `types.rs` (PromptTemplates struct), `PUT /config/prompts` with full replacement body |
| Add/update billing rule | `types.rs` (BillingData or nested structs), `PUT /config/billing` with full replacement body |
| Add detection threshold | `types.rs` (DetectionThresholds struct + `default_N()` fn + Default impl), `PUT /config/thresholds` |
| Add operational default | `types.rs` (extend `OperationalDefaults` struct + `default_N()` fn + `Default` impl + `validate()` bounds), `PUT /config/defaults` with full replacement body. Validation runs in `update_defaults()` before mutation; bad PUT returns 400 |
| Update server config | PUT to `/config/prompts`, `/config/billing`, `/config/thresholds`, or `/config/defaults`. Version auto-bumps. Clients re-fetch on next startup |

## ArchiveMetadata Notes

`ArchiveMetadata` in `types.rs` is the session metadata struct stored as `metadata.json`. Key fields include: session_id, started_at, ended_at, duration_ms, word_count, has_soap_note, has_audio, has_patient_handout, has_billing_record, charting_mode (`session`|`continuous`|`mobile`|`upload`), encounter_number, patient_name, detection_method, likely_non_clinical, patient_count, physician_id, physician_name, room_name, encounter_started_at.

**Schema-divergent fields**: The tauri-side `ArchiveMetadata` carries additional fields not declared on the profile-service struct (e.g., `patient_dob` from vision extraction in YYYY-MM-DD format, and any `shadow_comparison` data). The `PUT /sessions/:id/metadata` route was changed in v0.10.30 to deserialize the body as `serde_json::Value` (untyped) and call `patch_metadata()`, which merges non-null fields into the on-disk JSON. Result: unknown fields are preserved end-to-end without requiring the profile-service struct to be updated. So `patient_dob` round-trips correctly via tauri's `server_sync.rs::update_metadata()` even though the typed `ArchiveMetadata` struct here doesn't declare it.

The initial session creation path (`POST /sessions/upload`) still uses the typed `ArchiveMetadata` struct via `UploadSessionRequest`, so unknown fields ARE dropped on first upload — the subsequent metadata PUT (after enrichment) restores them. When adding a new field that needs server-side filtering, validation, or indexing, declare it on the typed struct here and bump the upload payload.
