# ADR-0013: LLM Router Migration (Ollama to OpenAI-Compatible API)

## Status

Accepted (Jan 12, 2025)

## Context

The app initially used Ollama's native API for LLM-based SOAP note generation and greeting detection. This approach had several limitations:

1. **Single-tenant**: Ollama runs locally, making multi-clinic deployments impractical
2. **No authentication**: No built-in auth mechanism for shared infrastructure
3. **Vendor lock-in**: Ollama-specific API endpoints limited model choices
4. **No routing**: Could not dynamically route to different models for different tasks

We needed an architecture that:
- Supports multi-tenant deployments with authentication
- Works with any LLM provider (OpenAI, Anthropic, local models)
- Allows different models for different tasks (SOAP vs. greeting check)
- Provides usage tracking per clinic/client

## Decision

Migrate from Ollama's native API to an **OpenAI-compatible LLM router API**.

### API Changes

| Aspect | Old (Ollama) | New (OpenAI-compatible) |
|--------|--------------|-------------------------|
| List models | `GET /api/tags` | `GET /v1/models` |
| Generate | `POST /api/generate` | `POST /v1/chat/completions` |
| Auth | None | `Authorization: Bearer <key>` |
| Client ID | N/A | `X-Client-Id: <client_id>` |
| Task type | N/A | `X-Clinic-Task: <task>` |

### Configuration Changes

| Old Setting | New Setting | Purpose |
|-------------|-------------|---------|
| `ollama_server_url` | `llm_router_url` | Router endpoint |
| `ollama_model` | `soap_model` | Model for SOAP generation |
| `ollama_keep_alive` | (removed) | Not needed with router |
| N/A | `llm_api_key` | Authentication |
| N/A | `llm_client_id` | Client/clinic identifier |
| N/A | `fast_model` | Model for quick tasks (greeting) |

### Request Format

**Old (Ollama)**:
```json
POST /api/generate
{
  "model": "qwen3:4b",
  "prompt": "Generate a SOAP note...",
  "stream": false
}
```

**New (OpenAI-compatible)**:
```json
POST /v1/chat/completions
Authorization: Bearer <api_key>
X-Client-Id: <client_id>
X-Clinic-Task: soap-generation

{
  "model": "gpt-4",
  "messages": [
    {"role": "system", "content": "You are a medical scribe..."},
    {"role": "user", "content": "Generate a SOAP note from this transcript..."}
  ],
  "stream": false
}
```

### Response Format

**Old (Ollama)**:
```json
{
  "response": "{\"subjective\": \"...\", ...}",
  "done": true
}
```

**New (OpenAI-compatible)**:
```json
{
  "choices": [
    {
      "message": {
        "content": "{\"subjective\": \"...\", ...}"
      }
    }
  ]
}
```

### Architecture

```
Frontend (React)
      │
      │ IPC (Tauri commands)
      ▼
┌─────────────────────────────────────┐
│ llm_client.rs                       │
│                                     │
│ ┌─────────────────────────────────┐ │
│ │ test_connection()               │ │  GET /v1/models
│ │ list_models()                   │──┼──────────────────┐
│ │ generate_soap_note()            │ │                   │
│ │ generate_multi_patient_soap()   │ │  POST /v1/chat/   │
│ │ check_greeting()                │──┼──completions     │
│ │ prewarm_model()                 │ │                   │
│ └─────────────────────────────────┘ │                   │
└─────────────────────────────────────┘                   │
                                                          ▼
                                        ┌─────────────────────────────┐
                                        │ LLM Router                  │
                                        │ (OpenAI-compatible API)     │
                                        │                             │
                                        │ - Authentication            │
                                        │ - Usage tracking per client │
                                        │ - Model routing             │
                                        │ - Rate limiting             │
                                        └─────────────────────────────┘
                                                          │
                    ┌─────────────────────────────────────┼─────────────────────────────────────┐
                    │                                     │                                     │
                    ▼                                     ▼                                     ▼
            ┌───────────────┐                     ┌───────────────┐                     ┌───────────────┐
            │ GPT-4         │                     │ Claude        │                     │ Local LLM     │
            │ (OpenAI)      │                     │ (Anthropic)   │                     │ (Ollama)      │
            └───────────────┘                     └───────────────┘                     └───────────────┘
```

### Implementation Details

**Backend (Rust)**:
- New `llm_client.rs` module with OpenAI-compatible client
- `ollama.rs` now re-exports from `llm_client.rs` for backward compatibility
- Exponential backoff retry for transient failures
- Custom headers for client identification and task tracking

**Frontend (TypeScript)**:
- `LLMStatus` type alias for backward compatibility (`OllamaStatus = LLMStatus`)
- Updated `useOllamaConnection` hook with new test_connection signature
- New settings fields in `useSettings` and `SettingsDrawer`
- Command names unchanged for backward compatibility

### Backward Compatibility

To minimize disruption, we maintained:
- Tauri command names (`check_ollama_status`, `list_ollama_models`, etc.)
- TypeScript type aliases (`OllamaStatus` = `LLMStatus`)
- Hook names (`useOllamaConnection`)
- File names (`ollama.rs` as re-export module)

Only configuration field names changed in `config.json`.

## Consequences

### Positive

- **Multi-tenant support**: Authentication and client IDs enable shared infrastructure
- **Provider flexibility**: Works with any OpenAI-compatible API (GPT-4, Claude, local models)
- **Task-specific models**: Use fast models for greeting checks, capable models for SOAP
- **Usage tracking**: `X-Client-Id` and `X-Clinic-Task` headers enable per-client analytics
- **Rate limiting**: Router can implement per-client rate limits
- **Industry standard**: OpenAI API is the de facto standard for LLM APIs
- **Future-proof**: Easy to add new models or switch providers

### Negative

- **Configuration migration**: Existing configs need manual update to new field names
- **Router dependency**: Requires LLM router infrastructure (additional service to maintain)
- **Network dependency**: Local-only Ollama deployments no longer work without router
- **API key management**: Need secure distribution of API keys to clinics

## Migration Guide

To migrate from Ollama to LLM router:

1. Update `~/.transcriptionapp/config.json`:
   ```json
   {
     "llm_router_url": "http://your-router:4000",
     "llm_api_key": "your-api-key",
     "llm_client_id": "clinic-001",
     "soap_model": "gpt-4",
     "fast_model": "gpt-3.5-turbo"
   }
   ```

2. Remove old settings:
   - `ollama_server_url`
   - `ollama_model`
   - `ollama_keep_alive`

3. Restart the app

## References

- [OpenAI API Reference](https://platform.openai.com/docs/api-reference/chat)
- [ADR-0009: LLM SOAP Note Generation](./0009-ollama-soap-generation.md)
- [ADR-0012: Multi-Patient SOAP Generation](./0012-multi-patient-soap-generation.md)
