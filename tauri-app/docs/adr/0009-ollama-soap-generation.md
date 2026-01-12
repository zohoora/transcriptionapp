# ADR-0009: LLM SOAP Note Generation

## Status

Accepted (Updated Jan 2025: Audio Events, Updated Jan 12 2025: OpenAI-compatible API)

**Note**: See ADR-0013 for the migration from Ollama native API to OpenAI-compatible LLM router.

## Context

Physicians spend significant time converting transcribed conversations into structured clinical documentation. SOAP (Subjective, Objective, Assessment, Plan) notes are a standard format. We need to:

1. Generate structured SOAP notes from unstructured transcripts
2. Keep data local (no cloud AI services for PHI)
3. Support different LLM models based on hardware capabilities
4. Provide reliable, parseable output
5. **[Jan 2025]** Include audio event context (coughs, laughs) for clinical relevance

## Decision

Integrate with **OpenAI-compatible LLM router** for LLM inference with JSON-structured output.

### Architecture
```
Transcript Text ────────────────────┐
                                    │
Audio Events (from YAMNet) ─────────┤
- Cough at 0:30 (88%)               │
- Laughter at 1:05 (82%)            ▼
                              ┌─────────────────┐
                              │ llm_client.rs   │
                              │ ┌─────────────┐ │     Authorization: Bearer <key>
                              │ │ HTTP Client │ │──>  X-Client-Id: <client_id>
                              │ └─────────────┘ │     X-Clinic-Task: soap-generation
                              │       │         │              │
                              │       ▼         │              ▼
                              │ POST /v1/chat/  │     LLM Router (OpenAI-compatible)
                              │   completions   │              │
                              │       │         │              ▼
                              │ ┌─────────────┐ │     LLM Response (GPT, Claude, etc.)
                              │ │JSON Parser  │<────────────────┘
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

TRANSCRIPT:
[transcript text]

AUDIO EVENTS DETECTED:
- Cough at 0:30 (confidence: 88%)
- Laughter at 1:05 (confidence: 82%)

Respond with ONLY valid JSON:
{
  "subjective": "...",
  "objective": "...",
  "assessment": "...",
  "plan": "..."
}

Rules:
- Only include information explicitly mentioned or directly inferable
- Use "No information available" if a section has no relevant content
- Consider audio events when relevant to the clinical picture
```

Key prompt features:
- `/no_think` prefix disables reasoning mode (Qwen, DeepSeek)
- Explicit JSON schema in prompt
- "ONLY" emphasis to minimize preamble/postamble
- Handles markdown code blocks in response (some models wrap JSON in ```)
- Handles `<think>` blocks if model ignores `/no_think`
- **[Jan 2025]** Audio events section when events are detected
- Confidence converted from logits to percentages (sigmoid mapping)

### Audio Events (Jan 2025)

Audio events detected by YAMNet (coughs, laughs, sneezes, throat clearing) are now included in SOAP generation:

```rust
pub struct AudioEvent {
    pub timestamp_ms: u64,    // When event occurred
    pub duration_ms: u32,     // Event duration
    pub confidence: f32,      // Raw logit from YAMNet
    pub label: String,        // "Cough", "Laughter", etc.
}
```

**Confidence Conversion:**
- YAMNet outputs logits (not probabilities)
- Converted to percentage: `100 / (1 + e^(-confidence))`
- Example: logit 1.5 → 82%, logit 2.0 → 88%, logit 3.0 → 95%

**Why include audio events?**
- Frequent coughing suggests respiratory issues
- Laughter may indicate patient comfort level
- Throat clearing could suggest voice strain
- Provides objective observations for SOAP note

**UI Decision:**
- Removed audio event display from UI (was showing in BiomarkersSection, RecordingMode)
- Events are recorded but only used for LLM context
- Clinician sees the final SOAP note, not raw event list

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

- Requires LLM router infrastructure (see ADR-0013)
- Generation takes 10-30 seconds depending on model/hardware
- LLM output not guaranteed to be medically accurate (for review only)

## References

- [OpenAI API Reference](https://platform.openai.com/docs/api-reference/chat) - API format used
- [ADR-0013: LLM Router Migration](./0013-llm-router-migration.md) - Migration to OpenAI-compatible API
- [SOAP Note Format](https://en.wikipedia.org/wiki/SOAP_note)
