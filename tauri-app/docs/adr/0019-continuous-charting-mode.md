# ADR 0019: Continuous Charting Mode (End of Day)

## Status
Accepted

## Context
Physicians using the transcription app must currently manually start and stop recording for each patient encounter. This creates workflow friction during busy clinic days with back-to-back patients. Some physicians prefer to review and finalize all documentation at the end of the day rather than after each visit.

## Decision
Implement a "Continuous Charting Mode" that:

1. **Records continuously** without manual start/stop between patients
2. **Auto-detects encounter boundaries** using an LLM to analyze the transcript
3. **Generates SOAP notes automatically** for each detected encounter
4. **Archives encounters separately** with metadata indicating auto-charted status
5. **Provides a monitoring dashboard** showing recording status, encounter count, and live transcript

### Architecture

```
Microphone → Pipeline (runs indefinitely) → TranscriptBuffer
                                                   ↓
                                            Encounter Detector (LLM)
                                                   ↓
                                            Complete encounter?
                                            YES → Archive + SOAP
                                            NO  → Continue buffering
```

### Key Components

**Backend (Rust)**:
- `TranscriptBuffer`: Thread-safe buffer accumulating timestamped segments
- `EncounterDetector`: LLM-based analysis to identify complete patient encounters
- `ContinuousModeHandle`: Orchestrates pipeline, buffer, and detector tasks
- Detection triggers: Periodic timer OR silence gap detection

**Frontend (React)**:
- `ContinuousMode.tsx`: Monitoring dashboard with live stats
- `useContinuousMode.ts`: Hook for events, status, and controls
- Settings toggle: "After Every Session" vs "End of Day"

### Detection Strategy

The encounter detector uses an LLM prompt that instructs the model to:
1. Identify if a complete patient encounter exists in the transcript
2. Look for: greeting/introduction → clinical discussion → farewell/wrap-up
3. Return the segment index where the encounter ends
4. Only mark complete if confident the encounter has ended

Detection triggers (whichever comes first):
- **Timer**: Every 2 minutes (configurable)
- **Silence**: After 60 seconds of continuous silence (configurable)
- **Buffer size**: Safety valve at ~5000 words

### Settings

```rust
charting_mode: String,               // "session" | "continuous"
continuous_auto_copy_soap: bool,     // Default: false (no clipboard spam)
encounter_check_interval_secs: u32,  // Default: 120
encounter_silence_trigger_secs: u32, // Default: 60
```

### Session Metadata

Continuous mode sessions include additional metadata:
- `charting_mode: "continuous"` - Identifies auto-charted sessions
- `encounter_number: u32` - Sequential number within the day

## Consequences

### Positive
- Zero workflow interruption during patient visits
- Physicians can batch-review documentation at end of day
- No missed recordings due to forgetting to start
- Automatic encounter segmentation reduces manual work

### Negative
- Higher LLM costs due to periodic encounter detection calls
- Potential for incorrect encounter boundaries (requires LLM accuracy)
- Longer transcript buffers consume more memory
- SOAP notes may need more manual review/editing

### Neutral
- Requires LLM connection for encounter detection (same as SOAP generation)
- History window shows both session-mode and continuous-mode encounters
- Auto-end detection disabled in continuous mode (pipeline never auto-stops)

## Alternatives Considered

1. **Silence-based segmentation**: Split encounters on long silence gaps
   - Rejected: Unreliable - silence during examination, brief pauses misinterpreted

2. **Keyword-based detection**: Look for "goodbye", "see you", etc.
   - Rejected: Too brittle, misses variations, false positives from quotes

3. **Fixed-interval sessions**: Auto-create sessions every N minutes
   - Rejected: Arbitrary boundaries don't match actual encounters

4. **Manual marking**: Button to mark encounter end
   - Rejected: Defeats the purpose of zero-interaction workflow

## Implementation Notes

- Pipeline runs with `auto_end_enabled: false` to prevent auto-stopping
- TranscriptBuffer uses monotonic segment indices for reliable drain operations
- Encounter detection uses `fast_model` alias (lightweight classification task)
- Frontend conditionally renders ContinuousMode vs Ready/Recording/Review modes
- History window displays "Auto-charted" badge and encounter numbers
