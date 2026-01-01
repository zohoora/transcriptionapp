# ADR-0009: Ollama SOAP Note Generation

## Status

Accepted

## Context

Physicians spend significant time converting transcribed conversations into structured clinical documentation. SOAP (Subjective, Objective, Assessment, Plan) notes are a standard format. We need to:

1. Generate structured SOAP notes from unstructured transcripts
2. Keep data local (no cloud AI services for PHI)
3. Support different LLM models based on hardware capabilities
4. Provide reliable, parseable output

## Decision

Integrate with **Ollama** for local LLM inference with JSON-structured output.

### Architecture
```
Transcript Text
      │
      ▼
┌─────────────────┐
│   ollama.rs     │
│ ┌─────────────┐ │
│ │ HTTP Client │ │──────> Ollama Server (localhost:11434)
│ └─────────────┘ │            │
│ ┌─────────────┐ │            ▼
│ │JSON Parser  │<────── LLM Response (Qwen, Llama, etc.)
│ └─────────────┘ │
└─────────────────┘
      │
      ▼
SoapNote { subjective, objective, assessment, plan }
```

### Prompt Engineering
Request **JSON output only** for reliable parsing:

```
/no_think You are a medical scribe assistant...

Respond with ONLY valid JSON:
{
  "subjective": "...",
  "objective": "...",
  "assessment": "...",
  "plan": "..."
}
```

Key prompt features:
- `/no_think` prefix disables reasoning mode (Qwen, DeepSeek)
- Explicit JSON schema in prompt
- "ONLY" emphasis to minimize preamble/postamble
- Handles markdown code blocks in response (some models wrap JSON in ```)
- Handles `<think>` blocks if model ignores `/no_think`

### Model Support
- Default: `qwen3:4b` (fast, good medical knowledge)
- Configurable via settings
- Connection status shown in UI

### Response Parsing
1. Strip `<think>...</think>` blocks
2. Extract JSON from markdown code fences if present
3. Parse with `serde_json`
4. Replace empty sections with "No information available."

## Consequences

### Positive

- Local inference (no PHI sent to cloud)
- Configurable models (trade speed vs. quality)
- JSON output provides structured, parseable data
- Handles quirks of different LLM outputs gracefully
- Type-safe parsing with serde

### Negative

- Requires Ollama installation (user setup step)
- Model download can be large (4GB+ for quality models)
- Generation takes 10-30 seconds depending on model/hardware
- LLM output not guaranteed to be medically accurate (for review only)

## References

- [Ollama API Documentation](https://github.com/ollama/ollama/blob/main/docs/api.md)
- [Qwen3 Model](https://huggingface.co/Qwen/Qwen3-4B)
- [SOAP Note Format](https://en.wikipedia.org/wiki/SOAP_note)
