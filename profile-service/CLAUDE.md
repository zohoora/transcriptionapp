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
│   ├── mod.rs         # build_router() — ~45 route registrations
│   ├── health.rs      # GET /health
│   ├── infrastructure.rs  # Clinic-wide settings (singleton)
│   ├── mobile.rs      # Mobile job upload, status, CRUD (6 handlers)
│   ├─��� physicians.rs  # Physician CRUD
│   ├── rooms.rs       # Room CRUD
│   ├── sessions.rs    # Session storage, split, merge, audio, files, day-log
│   └── speakers.rs    # Speaker profile CRUD
└── store/
    ├── mod.rs         # AppState (5 RwLock<Manager> + SessionStore)
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
| File allowlist | `is_allowed_session_file()` — explicit allowlist for auxiliary files (pipeline_log, replay_bundle, segments, screenshots/*.jpg, billing.json) |
| Input validation | `validate()` methods on all Create/Update request types — name 500 chars, instructions 10K, etc. |
| Backup safety | Split/merge operations create `.bak` files before modifying transcripts, removed on success |
| JSON stores | Physicians, rooms, speakers, infrastructure — in-memory Vec + `atomic_write` to disk on mutation. `0o600` file permissions |
| Mobile job idempotency | `recording_index: HashMap<String, String>` — O(1) lookup by `recording_id` to prevent duplicate job creation on upload retry |
| Mobile job persistence | In-memory `HashMap<String, MobileJob>` + atomic JSON write on every mutation. Audio files stored as `{job_id}.m4a` in `mobile_uploads/` |
| axum path params | axum 0.7 uses `:id` syntax (NOT `{id}`) |

## API Routes

~45 route handlers across 7 resource types. Session routes scoped under `/physicians/:id/sessions/...`. Mobile routes under `/mobile/...`.

| Resource | Endpoints |
|----------|-----------|
| Health | `GET /health` |
| Infrastructure | `GET/PUT /infrastructure` |
| Mobile Jobs | `POST /mobile/upload`, `GET /mobile/jobs`, `GET/PUT/DELETE /mobile/jobs/:job_id`, `GET /mobile/uploads/:job_id` |
| Physicians | `GET/POST /physicians`, `GET/PUT/DELETE /physicians/:id` |
| Rooms | `GET/POST /rooms`, `GET/PUT/DELETE /rooms/:id` |
| Speakers | `GET/POST /speakers`, `GET/PUT/DELETE /speakers/:id` |
| Sessions | dates, list, get, upload, delete, split, merge, renumber, metadata, soap, feedback, patient-name, transcript-lines, audio, files, screenshots, day-log |

## Data Storage

File-based, under `~/.fabricscribe/` (configurable via `--data-dir`):

```
profile-data/
├── physicians.json          # All physician profiles
├── rooms.json               # All room configs
├── speakers.json            # Speaker voice profiles
├── infrastructure.json      # Clinic-wide settings
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
| Modify file allowlist | `store/sessions.rs` (`is_allowed_session_file()`) — currently allows: pipeline_log, replay_bundle, segments, screenshots/*.jpg, billing.json |

## ArchiveMetadata Notes

`ArchiveMetadata` in `types.rs` is the session metadata struct stored as `metadata.json`. Key fields include: session_id, started_at, ended_at, duration_ms, word_count, has_soap_note, has_audio, has_patient_handout, has_billing_record, charting_mode, encounter_number, patient_name, detection_method, likely_non_clinical, patient_count, physician_id, physician_name, room_name, encounter_started_at.

**Note**: The tauri app's ArchiveMetadata includes `patient_dob` (vision-extracted date of birth, `Option<String>` in YYYY-MM-DD format) which is not yet in the profile service struct. The field is silently dropped during metadata uploads due to `#[serde(default)]`.
