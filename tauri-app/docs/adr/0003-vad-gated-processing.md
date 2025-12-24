# ADR-0003: VAD-Gated Audio Processing

## Status

Accepted

## Context

Whisper expects complete utterances for best results, not continuous audio streams. We need a strategy to:

- Detect when the user is speaking
- Accumulate speech into complete utterances
- Send utterances to Whisper at appropriate boundaries
- Minimize latency while maximizing transcription quality

Options considered:
1. **Fixed-interval chunking** - Simple but ignores speech boundaries
2. **VAD-gated chunking** - Uses Voice Activity Detection to find natural breaks
3. **Streaming Whisper** - Not well supported, quality issues

## Decision

We chose **VAD-gated audio processing** using Silero VAD.

The pipeline:
1. Audio samples flow continuously from the input device
2. Silero VAD analyzes each chunk for speech probability
3. Speech is accumulated into an utterance buffer
4. When silence is detected, the utterance is sent to Whisper
5. Pre-roll audio captures word beginnings that might be clipped

Configuration parameters:
- `vad_threshold`: Speech probability threshold (0.0-1.0)
- `pre_roll_ms`: Audio to keep before speech detection
- `min_speech_ms`: Minimum speech duration before triggering
- `silence_to_finalize_ms`: Silence duration to end utterance
- `max_utterance_ms`: Maximum utterance length (force flush)

## Consequences

### Positive

- Natural speech boundaries improve transcription accuracy
- Reduces Whisper processing overhead (only process speech)
- Configurable sensitivity for different use cases
- Pre-roll prevents clipping word beginnings

### Negative

- Additional processing latency from VAD
- Requires tuning for different environments
- VAD can make mistakes in noisy environments
- More complex than fixed-interval approach

## References

- [Silero VAD](https://github.com/snakers4/silero-vad)
- [voice-activity-detector crate](https://crates.io/crates/voice-activity-detector)
