# ADR-0020: STT Router Streaming Integration

## Status

Accepted (Feb 2026)

**Supersedes**: ADR-0002 (Whisper for Transcription)

## Context

The app previously used direct HTTP batch requests to a faster-whisper-server (`/v1/audio/transcriptions`). This had several limitations:

1. **High latency**: Batch mode requires the full utterance before transcription begins (~48x realtime)
2. **No alias routing**: Could not dynamically select different STT backends or configurations
3. **No post-processing**: Medical term correction required separate processing
4. **Single protocol**: Only OpenAI-compatible batch endpoint, no streaming support

The STT Router provides a unified gateway with:
- Named aliases mapping to different model/pipeline configurations
- WebSocket streaming for real-time partial results (~2x realtime)
- Built-in medical term post-processing
- Health and alias discovery endpoints

## Decision

Migrate all transcription modes from batch HTTP to **WebSocket streaming via STT Router**.

### Protocol

1. **Connect**: WebSocket to `ws://<host>:<port>/v1/audio/stream`
2. **Configure**: Send JSON `{"alias": "medical-streaming", "postprocess": true}`
3. **Stream audio**: Send binary WAV data (16kHz, mono, 16-bit PCM)
4. **Receive chunks**: `{"type": "transcript_chunk", "text": "partial..."}` messages arrive in real-time
5. **Final result**: `{"type": "transcript_final", "text": "complete transcription"}` on close

### Configuration

| Setting | Default | Purpose |
|---------|---------|---------|
| `whisper_server_url` | `http://10.241.15.154:8001` | STT Router base URL |
| `stt_alias` | `medical-streaming` | Named alias for model/pipeline selection |
| `stt_postprocess` | `true` | Enable medical term post-processing |

### Available Aliases

Aliases are discovered via `GET /v1/aliases`:

| Alias | Backend | Use Case |
|-------|---------|----------|
| `medical-streaming` | Voxtral/Whisper | Clinical transcription (streaming) |
| `medical-batch` | Whisper | Clinical transcription (batch fallback) |
| `regular-streaming` | Voxtral/Whisper | General transcription (streaming) |
| `regular-batch` | Whisper | General transcription (batch fallback) |

### Modes Updated

All three recording modes now use WebSocket streaming:

1. **Session mode** (`pipeline.rs`): `transcribe_utterance()` calls `transcribe_streaming_blocking()`, emits `TranscriptChunk` messages for real-time preview
2. **Continuous mode** (`continuous_mode.rs`): Same streaming path, emits `continuous_transcript_preview` events
3. **Listening mode** (`listening.rs`): `analyze_speech()` uses `transcribe_streaming_blocking()` for greeting detection

### Pipeline Message

New `TranscriptChunk { text: String }` variant in `PipelineMessage` delivers partial results to the frontend via `draft_text` field in `TranscriptUpdate`.

### E2E Testing

Integration tests in `e2e_tests.rs` validate the full pipeline across 5 layers:

| Layer | Tests | Validates |
|-------|-------|-----------|
| 1 | STT health, alias, streaming | Router connectivity and protocol |
| 2 | LLM SOAP, encounter detection | LLM Router integration |
| 3 | Archive save/retrieve | Local storage |
| 4 | Session mode full E2E | Audio -> STT -> SOAP -> Archive -> History |
| 5 | Continuous mode full E2E | Audio -> STT -> Detection -> SOAP -> Archive -> History |

Run with: `cargo test e2e_ -- --ignored --nocapture`

## Consequences

### Positive

- **Real-time feedback**: Streaming delivers partial transcripts as speech happens
- **Medical optimization**: `medical-streaming` alias routes to models tuned for clinical terminology
- **Post-processing**: Server-side medical term correction improves accuracy
- **Unified gateway**: Single STT Router URL for all transcription needs
- **Alias flexibility**: Switch models by changing config, no code changes needed

### Negative

- **Network dependency**: All transcription requires STT Router (no offline fallback)
- **WebSocket complexity**: Streaming protocol more complex than batch HTTP
- **tungstenite dependency**: Uses synchronous WebSocket client (pipeline runs on blocking threads)

### Migration

The legacy `transcribe()` and `transcribe_blocking()` methods still exist in `whisper_server.rs` but have no callers. They can be removed in a future cleanup.

## References

- [STT Router Integration Guide](file:///Users/backoffice/Library/CloudStorage/Dropbox/STT_ROUTER_INTEGRATION.md) - Protocol specification
- ADR-0002: Whisper for Transcription (superseded)
- ADR-0013: LLM Router Migration - Similar pattern for LLM routing
