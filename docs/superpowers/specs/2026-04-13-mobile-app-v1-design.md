# AMI Assist Mobile v1 — Design Spec

## Context

Physicians at the clinic occasionally make house calls where they're away from the clinic workstations running AMI Assist. During these visits, there's no ambient scribe — the physician must take notes manually. The mobile app solves this by letting physicians record house call appointments on their iPhone, then have the recording automatically processed (STT + encounter detection + SOAP) when they return to the clinic network. This extends AMI Assist's coverage beyond the clinic walls without requiring any on-device ML or real-time processing.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Platform | iOS (Swift/SwiftUI) | Native audio APIs, best battery life, background recording support |
| Offline strategy | Record only, process on server | Simplest mobile app, no on-device models (~0MB ML overhead) |
| Audio format | AAC/m4a, 16kHz mono | ~7MB/hour vs ~115MB/hour for WAV. Server transcodes to WAV for STT |
| Processing | CLI binary sharing desktop app's Rust modules | Zero algorithm divergence — same `llm_client.rs`, `encounter_detection.rs` code |
| Job coordination | Profile service tracks job status | Stays a dumb storage/coordination layer, no STT/LLM logic |
| Integration | Same profile service (port 8090) | Sessions appear in desktop app automatically |
| Encounter handling | Batch auto-split via CLI | Reuses existing encounter detection logic verbatim |
| Review UX | Status display only | SOAP review happens on the desktop app |
| Auth | One-time physician selection | Same trust model as desktop clinic workstations |
| Repo location | `ios/` directory in transcriptionapp monorepo | Coordinated releases, shared context |

## System Architecture

```
┌─────────────────┐         ┌───────────────────────────────────────────────┐
│   iOS App        │         │  MacBook Server                               │
│   (SwiftUI)      │         │                                               │
│                  │  HTTP   │  Profile Service (:8090)                      │
│  Record audio ───┼────────►│    ├─ Existing routes (physicians, sessions)  │
│  Upload AAC   ───┼────────►│    ├─ POST /mobile/upload  (NEW, stores job) │
│  Poll status  ───┼────────►│    ├─ GET  /mobile/jobs/*  (NEW, job status) │
│                  │         │    └─ Job storage (queue + audio files)       │
│                  │         │                                               │
│                  │         │  Processing CLI (process_mobile)              │
│                  │         │    ├─ Polls profile service for queued jobs   │
│                  │         │    ├─ ffmpeg transcode (AAC→WAV)             │
│                  │         │    ├─ STT Router (:8001) via WebSocket       │
│                  │         │    ├─ Encounter Detection (shared Rust code) │
│                  │         │    ├─ SOAP Generation (shared Rust code)     │
│                  │         │    └─ Uploads results to profile service     │
│                  │         │                                               │
│                  │         │  STT Router (:8001)                           │
│                  │         │  LLM Router (:8080)                           │
└─────────────────┘         └───────────────────────────────────────────────┘
```

**Three components:**
1. **iOS App** — thin client: records audio, uploads to profile service, shows job status
2. **Profile Service** — coordination layer: stores audio + job metadata, serves status to mobile app. No processing logic.
3. **Processing CLI** (`process_mobile`) — intelligence layer: imports the same Rust modules as the desktop app. Polls for jobs, runs the full pipeline, writes results back. Algorithm changes in the desktop codebase automatically apply here.

## Component 1: iOS App

### Screens (4 total)

#### 1. Setup Screen (first launch only)
- Text field for server URL (Tailscale IP pre-filled as placeholder: `http://100.119.83.76:8090`)
- Fetches physician list from `GET /physicians`
- Physician selection stored in UserDefaults
- "Connect" button validates server reachability before saving
- Not shown again unless physician taps "Change Physician" in Settings

#### 2. Recording Screen (main screen)
- Large circular record/stop button (center)
- Timer showing elapsed recording duration
- Audio level meter (thin bar, uses `AVAudioRecorder.averagePower(forChannel:)`)
- Physician name at top
- States: "Ready" → "Recording..." → "Saving..."
- Navigation to Sessions List and Settings via tab bar or toolbar

#### 3. Sessions List Screen
- Table/List of recordings sorted by date (newest first)
- Each row: date/time, duration, status badge
- Status badges with colors:
  - `Saved` (gray) — recorded locally, not yet uploaded
  - `Uploading` (blue) — upload in progress
  - `Processing` (yellow) — server is processing
  - `Complete` (green) — STT + SOAP done
  - `Failed` (red) — processing error, tap to retry
- Pull-to-refresh: re-fetches job statuses from server
- Swipe-to-delete for local recordings (confirmation alert for uploaded ones)

#### 4. Settings Screen
- Server URL (editable)
- Current physician name + "Change" button
- Storage usage: total MB of local recordings
- "Upload All" button: manually triggers upload of all pending recordings
- "Delete Uploaded" button: clears local copies of successfully uploaded recordings
- App version

### Recording Implementation

**Framework:** `AVAudioRecorder`

**Audio session configuration:**
```swift
let session = AVAudioSession.sharedInstance()
try session.setCategory(.record, mode: .default)
try session.setActive(true)
```

**Recorder settings:**
```swift
let settings: [String: Any] = [
    AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
    AVSampleRateKey: 16000.0,
    AVNumberOfChannelsKey: 1,
    AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue
]
```

**Background recording:**
- `UIBackgroundModes: [audio]` in Info.plist
- AVAudioSession keeps the app alive while recording
- Recording persists through screen lock and app backgrounding

**Local storage:**
```
Documents/
├── recordings/
│   ├── {uuid}.m4a          # Audio file
│   ├── {uuid}.json         # Metadata sidecar
│   └── ...
```

**Metadata sidecar (`{uuid}.json`):**
```json
{
    "recording_id": "uuid",
    "physician_id": "uuid",
    "physician_name": "Dr. Smith",
    "started_at": "2026-04-13T10:30:00Z",
    "duration_ms": 1800000,
    "status": "saved",
    "job_id": null,
    "uploaded_at": null
}
```

### Upload Implementation

**Trigger:** `NWPathMonitor` detects network connectivity → scan for recordings with `status: "saved"` → upload sequentially.

**Mechanism:** `URLSession` background upload task (survives app suspension).

**Request:**
```
POST /mobile/upload
Content-Type: multipart/form-data

Fields:
  - audio: file (AAC/m4a binary)
  - physician_id: "uuid"
  - started_at: "2026-04-13T10:30:00Z"
  - duration_ms: 1800000
  - recording_id: "uuid" (for idempotency)
  - device_info: "iPhone 15, iOS 18.4" (optional)
```

**Response:**
```json
{
    "job_id": "uuid",
    "status": "queued"
}
```

**On success:** Update local metadata: `status → "uploading"` then `"processing"`, store `job_id`.

**On failure:** Retry with exponential backoff (3 attempts). After 3 failures, mark as "saved" (will retry on next connectivity event).

### Status Polling

When app is in foreground and has jobs in `processing` state:
- Poll `GET /mobile/jobs/{job_id}` every 10 seconds
- Update local metadata with server status
- Stop polling when status reaches `complete` or `failed`

No polling when app is backgrounded (saves battery). Refresh on app foreground via `scenePhase` observer.

## Component 2: Profile Service — Job Tracking (Storage Only)

The profile service gains mobile upload and job tracking endpoints but **no processing logic**. It stores audio files and job metadata, serving as a coordination point between the iOS app and the processing CLI.

### New API Endpoints (`routes/mobile.rs`)

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/mobile/upload` | Upload audio + metadata, create job, return job_id |
| `GET` | `/mobile/jobs/{job_id}` | Get single job status |
| `GET` | `/mobile/jobs?physician_id={id}` | List all jobs for a physician |
| `GET` | `/mobile/jobs?status=queued` | List jobs by status (used by processing CLI to find work) |
| `PUT` | `/mobile/jobs/{job_id}` | Update job status + results (used by processing CLI) |
| `DELETE` | `/mobile/jobs/{job_id}` | Cancel/delete a pending job |
| `GET` | `/mobile/uploads/{job_id}` | Download uploaded audio file (used by processing CLI) |

### Job Data Model

```rust
struct MobileJob {
    job_id: String,
    physician_id: String,
    recording_id: String,        // for idempotency (from iOS app)
    started_at: String,          // RFC3339 — when recording started
    duration_ms: u64,
    status: JobStatus,
    error: Option<String>,
    sessions_created: Vec<CreatedSession>,
    created_at: String,          // when job was created
    updated_at: String,          // last status change
}

enum JobStatus {
    Queued,          // uploaded, waiting for CLI to pick up
    Transcoding,     // CLI is transcoding AAC→WAV
    Transcribing,    // CLI is running STT
    Detecting,       // CLI is running encounter detection
    GeneratingSoap,  // CLI is generating SOAP notes
    Complete,        // all done, sessions created
    Failed,          // error occurred (see error field)
}

struct CreatedSession {
    session_id: String,
    encounter_number: u32,
    word_count: usize,
    has_soap: bool,
}
```

### Storage Layout

```
{profile_service_data}/
├── mobile_uploads/
│   ├── {job_id}.m4a            # Uploaded audio file
│   └── ...
├── mobile_jobs.json            # All job metadata (persisted on change)
├── sessions/                   # Existing session storage (unchanged)
│   └── {physician_id}/...
```

### Upload Handler Flow

1. Validate `physician_id` exists in physician store
2. Check `recording_id` for idempotency — if duplicate, return existing job
3. Generate `job_id` (UUID)
4. Save audio file to `mobile_uploads/{job_id}.m4a`
5. Create `MobileJob` with `status: Queued`
6. Persist to `mobile_jobs.json`
7. Return `{job_id, status: "queued"}`

## Component 3: Processing CLI (`process_mobile`)

A headless CLI binary that shares the same Rust source code as the desktop app. It polls the profile service for queued jobs, processes them using the existing pipeline modules, and writes results back.

### Binary Location

```
tauri-app/src-tauri/
├── src/
│   ├── llm_client.rs              # SHARED: SOAP prompts, LLM communication
│   ├── encounter_detection.rs     # SHARED: encounter split logic
│   ├── whisper_server.rs          # SHARED: STT WebSocket protocol
│   ├── transcription.rs           # SHARED: Segment/transcript types
│   ├── config.rs                  # SHARED: configuration types
│   └── ...
├── src/bin/
│   └── process_mobile.rs          # NEW: CLI entry point
├── Cargo.toml                     # Add [[bin]] entry for process_mobile
```

**Key advantage:** `process_mobile` imports modules like `llm_client`, `encounter_detection`, `whisper_server` directly. When you tweak SOAP prompts or detection thresholds in the desktop app code, the CLI binary picks up those changes on next `cargo build`.

### CLI Behavior

```
process_mobile --profile-service-url http://localhost:8090 \
               --stt-url http://localhost:8001 \
               --llm-url http://localhost:8080 \
               [--poll-interval 10] \
               [--once]  # process one job and exit (for testing)
```

**Main loop:**
1. Poll `GET /mobile/jobs?status=queued` every `poll_interval` seconds (default: 10)
2. If a queued job is found, claim it by `PUT /mobile/jobs/{id}` with `status: transcoding`
3. Download audio: `GET /mobile/uploads/{job_id}` → save to temp file
4. Run pipeline (transcode → STT → detect → SOAP)
5. Upload results to profile service (create sessions via existing session API)
6. Update job: `PUT /mobile/jobs/{id}` with `status: complete` + `sessions_created`
7. On error: `PUT /mobile/jobs/{id}` with `status: failed` + `error` message
8. Loop back to step 1

### Pipeline Steps (reusing desktop code)

**Step 1: Transcode**
```bash
ffmpeg -i input.m4a -ar 16000 -ac 1 -f wav output.wav
```
Invoked via `std::process::Command`. Requires ffmpeg in PATH.

**Step 2: Speech-to-Text**
Reuses `whisper_server.rs` WebSocket protocol:
- Connect to `ws://{stt_url}/v1/audio/stream`
- Send config: `{"alias": "medical-streaming", "postprocess": true}`
- Stream entire WAV as binary frames (32KB chunks) — same protocol as real-time streaming, but sending the complete file at once. STT Router handles both patterns.
- Collect transcript chunks, return final transcript text
- Timeout: `max(120s, duration_ms / 1000 * 1.5)` — longer recordings need more time

**Step 3: Encounter Detection**
Reuses `encounter_detection.rs` logic directly:
- < 500 words → single session, skip detection
- 500–3000 words → call `build_encounter_detection_prompt()` once on full transcript
- 3000+ words → sequential scan in ~2000-word windows (200-word overlap), collect split points
- Confidence gate: >= 0.8 (slightly lower than desktop real-time since we have full context)
- Force split at 25,000 words (matching `ABSOLUTE_WORD_CAP`)
- Creates sessions with `charting_mode: "mobile"`, `detection_method: "batch_llm"`

**Step 4: SOAP Generation**
Reuses `llm_client.rs` directly:
- Calls `build_simple_soap_prompt()` with physician's SOAP preferences (fetched from profile service)
- Calls `build_soap_user_content()` with transcript segment
- POSTs to LLM Router at `/v1/chat/completions` with `model: "soap-model-fast"`
- Parses SOAP JSON response using existing `parse_soap_with_retry()` logic
- Timeout: 300 seconds (matching `SOAP_GENERATION_TIMEOUT_SECS`)

**Step 5: Store Results**
- Creates session in profile service via `POST /physicians/{id}/sessions/{session_id}`
- Uploads transcript.txt, metadata.json, soap_note.txt
- Same format as desktop-created sessions — desktop app sees them seamlessly

### Running as a Daemon

On the MacBook, run the CLI as a background process (or launchd service):
```bash
# Simple background run
nohup ./process_mobile --profile-service-url http://localhost:8090 \
                       --stt-url http://localhost:8001 \
                       --llm-url http://localhost:8080 &

# Or via launchd plist for auto-restart
```

### Config

The CLI takes all configuration via command-line args or environment variables:

| Flag | Env Var | Default | Purpose |
|------|---------|---------|---------|
| `--profile-service-url` | `PROFILE_SERVICE_URL` | `http://localhost:8090` | Profile service base URL |
| `--stt-url` | `STT_ROUTER_URL` | `http://localhost:8001` | STT Router URL |
| `--llm-url` | `LLM_ROUTER_URL` | `http://localhost:8080` | LLM Router URL |
| `--llm-api-key` | `LLM_API_KEY` | none | Bearer token for LLM Router |
| `--poll-interval` | `POLL_INTERVAL_SECS` | `10` | Seconds between job polls |
| `--once` | — | false | Process one job and exit |

**Physician SOAP settings:** Fetched from profile service per job: `GET /physicians/{physician_id}` → extract `soap_detail_level`, `soap_format`, `soap_custom_instructions`.

## Encounter Detection Strategy

For mobile-recorded audio (batch processing, not real-time):

1. **Transcribe entire audio** into a single transcript
2. **Word count check:**
   - < 500 words → single session, no split detection needed
   - 500–3000 words → run encounter detection once on full transcript
   - 3000+ words → sequential scan: send transcript in ~2000-word windows (with 200-word overlap) to the LLM, asking each window if a patient transition occurs within it. Collect all detected split points, then partition the transcript at those boundaries.
3. **Split detection prompt:** Same `build_encounter_detection_prompt()` as desktop, but framed as "find transition points in this transcript segment." Expected response: `{"split": true, "confidence": 0.92, "split_point_line": 47}` or `{"split": false}`.
4. **Confidence gate:** Require confidence >= 0.8 for a split (slightly lower than desktop's real-time threshold since we have full context).
5. **Force split:** If transcript > 25,000 words (matching desktop's `ABSOLUTE_WORD_CAP`), split at the 25K boundary.
6. **Create sessions:** For each detected encounter, create a session in the existing session store with `charting_mode: "mobile"`, sequential `encounter_number`, `detection_method: "batch_llm"`.

This differs from the desktop's real-time detection (timer-based checks during recording) because we have the complete transcript upfront. The sequential scan approach is more accurate than the desktop's periodic timer checks.

## Error Handling

| Error | Behavior |
|-------|----------|
| Upload interrupted | iOS Background URLSession retries automatically. Server rejects duplicate `recording_id` |
| ffmpeg not found | CLI logs error, marks job as `Failed` with "ffmpeg not found" |
| STT Router down | 3 retries with exponential backoff (1s, 2s, 4s). Then `Failed` with retryable flag |
| LLM Router down | 3 retries. SOAP fails but transcript is preserved. Session created without SOAP |
| Very long audio (3+ hours) | STT processes in chunks. Detection handles transcript >25K words via force-split |
| Corrupt audio file | ffmpeg fails → job marked `Failed` with error message |
| Disk full | Upload rejected with 507. CLI job fails on write |
| Profile service restart | Job metadata persisted in `mobile_jobs.json`. CLI re-polls and finds incomplete jobs |
| CLI crash mid-processing | Job stays in non-terminal status. On CLI restart, re-polls and re-processes (idempotent steps) |
| CLI not running | Jobs accumulate in `queued` status. Mobile app shows "Processing" indefinitely. Physician can check Settings or CLI logs |

## What's NOT in v1

- No push notifications (poll-based status only)
- No on-device STT or ML models
- No real-time transcript during recording
- No patient handout generation from mobile
- No billing from mobile
- No SOAP editing on mobile (review on desktop)
- No speaker diarization labels (server may add later via STT Router)
- No multi-device concurrent recording per physician
- No offline physician profile editing
- No audio playback on mobile
- No Medplum/EMR sync from mobile

## Verification Plan

### iOS App Testing
1. Record a 5-minute audio with speech → verify .m4a file created in Documents/recordings/
2. Kill and reopen app → verify recording persists in Sessions List
3. Lock screen during recording → verify recording continues (background mode)
4. Connect to clinic WiFi → verify auto-upload triggers
5. Airplane mode during upload → verify retry on reconnect
6. Pull-to-refresh on Sessions List → verify status updates from server

### Profile Service Testing
1. `curl -F audio=@test.m4a -F physician_id=... /mobile/upload` → verify job_id returned, audio saved
2. `curl /mobile/jobs?status=queued` → verify job appears
3. `curl -X PUT /mobile/jobs/{id} -d '{"status":"complete"}'` → verify status updates
4. Duplicate upload with same recording_id → verify idempotent response

### Processing CLI Testing
1. `./process_mobile --once` with a queued job → verify full pipeline runs
2. Check `sessions/{physician_id}/` → verify transcript.txt and soap_note.txt created
3. Open desktop app → verify mobile-recorded session appears in History
4. Upload 2-hour recording with multiple encounters → verify auto-split creates multiple sessions
5. Kill CLI mid-processing → restart → verify job re-processed successfully

### Integration Testing
1. Full flow: record on phone → upload → CLI processes → view on desktop
2. Multiple recordings queued → verify sequential processing
3. Duplicate upload (same recording_id) → verify idempotent (no duplicate job)
4. Tweak SOAP prompt in desktop code → rebuild CLI → verify new prompt used for next job
