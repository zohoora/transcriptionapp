# ADR 0018: MIIS Medical Illustration Integration

## Status
Accepted

## Context
During clinical encounters, physicians often want to show patients relevant anatomical diagrams or medical illustrations to aid explanation. Currently, clinicians must manually search for and display these images, interrupting the flow of the consultation.

We have access to a Medical Illustration Image Server (MIIS) on the local network that can provide contextually relevant medical images based on the topics being discussed in real-time.

## Decision
Integrate MIIS to automatically suggest relevant medical illustrations during recording sessions.

### Architecture

```
Transcript → LLM Concept Extraction → MIIS Server → Image Suggestions → UI Display
                    ↓
            (Piggybacks on predictive hint call)
```

### Key Design Choices

1. **Concept Extraction via LLM**: Rather than keyword matching, we use the LLM to extract de-identified medical concepts from the transcript. This happens during the existing predictive hint generation (every 30 seconds), avoiding additional LLM calls.

2. **Rust Proxy for HTTP**: All MIIS requests go through Rust commands to avoid browser CORS restrictions. The frontend calls `miis_suggest` and `miis_send_usage` commands.

3. **Telemetry Batching**: Usage events (impressions, clicks, dismisses) are batched and sent every 5 seconds to reduce network overhead and improve ranking over time.

4. **CSP Configuration**: Tauri's Content Security Policy updated to allow image loading from MIIS server domain.

5. **Configurable**: MIIS can be enabled/disabled and server URL configured in settings.

### API Integration

**Suggest Endpoint** (`POST /v5/ambient/suggest`):
```json
{
  "session_id": "uuid",
  "concepts": [
    {"text": "knee anatomy", "weight": 0.9},
    {"text": "meniscus tear", "weight": 0.7}
  ],
  "limit": 6
}
```

**Usage Endpoint** (`POST /v5/usage`):
```json
{
  "session_id": "uuid",
  "suggestion_set_id": "uuid",
  "events": [
    {"image_id": 42, "type": "impression", "timestamp": "..."},
    {"image_id": 42, "type": "click", "timestamp": "..."}
  ]
}
```

### UI Components

- `ImageSuggestions.tsx`: Horizontal thumbnail strip with click-to-expand modal
- Thumbnails show dismiss button on hover
- Expanded view shows title, description, and full-size image

## Consequences

### Positive
- Physicians can quickly show relevant illustrations during consultation
- No manual searching required - images suggested automatically
- Usage telemetry improves ranking over time
- Minimal performance impact (piggybacks on existing LLM call)

### Negative
- Requires MIIS server on local network
- Image relevance depends on server's embedder quality
- Additional network traffic during recording sessions

### Dependencies
- MIIS server with embedder enabled for semantic matching
- LLM router for concept extraction
- Network access to MIIS server from client

## Files Changed
- `src-tauri/src/commands/miis.rs` - Rust proxy commands
- `src-tauri/src/commands/ollama.rs` - Added concept extraction to predictive hints
- `src-tauri/src/config.rs` - MIIS settings
- `src-tauri/tauri.conf.json` - CSP for image loading
- `src/hooks/usePredictiveHint.ts` - Returns concepts alongside hints
- `src/hooks/useMiisImages.ts` - MIIS suggestion fetching and telemetry
- `src/components/ImageSuggestions.tsx` - Thumbnail strip UI
- `src/components/modes/RecordingMode.tsx` - Integration with recording view
