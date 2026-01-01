# ADR-0007: Biomarker Analysis

## Status

Accepted

## Context

Clinical transcription can benefit from objective vocal biomarkers that provide insights into patient health beyond the words spoken. Physicians need tools to detect:

1. **Affect/Mood indicators** - Flat affect may indicate depression, PTSD, or other conditions
2. **Neurological markers** - Voice tremor or instability may indicate Parkinson's, fatigue, or other neurological conditions
3. **Respiratory events** - Coughs, wheezes, or throat clearing provide diagnostic information
4. **Conversation dynamics** - Turn-taking patterns, interruptions, and response latency indicate engagement and communication quality

These metrics should be computed in real-time without impacting transcription latency.

## Decision

Implement a parallel biomarker analysis system with three tiers:

### 1. Pure-Rust Metrics (No ONNX)
- **Vitality (F0 variability)**: Use `pitch-detection` crate (mcleod algorithm) to measure pitch standard deviation. Low F0 variability (<20 Hz std dev) indicates flat affect.
- **Stability (CPP)**: Use `rustfft` for cepstral analysis to compute Cepstral Peak Prominence. Low CPP (<6 dB) indicates vocal instability.

### 2. ONNX-based Detection
- **YAMNet**: 521-class audio event classifier (~3MB). Detects coughs, sneezes, throat clearing, and other clinically relevant sounds.
- **Emotion (wav2small)**: Dimensional emotion (Arousal, Dominance, Valence) already integrated in main pipeline.

### 3. Session Metrics
- **Conversation dynamics**: Computed from transcript segment timing (no additional audio processing)
- **Turn-taking statistics**: Count, duration, overlap, response latency
- **Talk-time ratios**: Per-speaker talk time when diarization enabled

### Architecture
```
Audio Pipeline
      │
      ├─────────────────────> Biomarker Sidecar Thread
      │ (clone after resample)      │
      │                             ├── YAMNet (all audio)
      │                             ├── Vitality (per utterance)
      │                             └── Stability (per utterance)
      │
      ▼
VAD → Whisper → Diarization → Transcript
                                   │
                                   ▼
                         Session Metrics Aggregator
                         (turn-taking, dynamics)
```

### Feature Flags
- `biomarkers` Cargo feature gates YAMNet and CPP stability
- Vitality uses `pitch-detection` (always available)
- Session metrics require no external dependencies

## Consequences

### Positive

- Real-time objective health markers without additional hardware
- Non-blocking: biomarker processing doesn't affect transcription latency
- Modular: each metric can be enabled/disabled independently
- Pure-Rust implementations for vitality/stability avoid ONNX dependency for basic metrics
- CPP (Cepstral Peak Prominence) is more robust than jitter/shimmer in ambient noise conditions

### Negative

- Additional CPU usage for parallel processing thread
- YAMNet model adds ~3MB to distribution size
- Biomarker interpretation requires clinical expertise (values are not diagnoses)
- Some false positives in noisy environments

## References

- [Silero VAD](https://github.com/snakers4/silero-vad) - Used for voice activity detection
- [YAMNet](https://tfhub.dev/google/yamnet/1) - Audio event classification
- [McLeod Pitch Algorithm](https://www.cs.otago.ac.nz/research/publications/oucs-2008-03.pdf) - F0 detection
- [Cepstral Peak Prominence](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC3689894/) - Voice quality metric
- [wav2small](https://github.com/audeering/w2v2-how-to) - Dimensional emotion detection
