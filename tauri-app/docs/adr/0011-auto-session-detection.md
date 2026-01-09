# ADR-0011: Auto-Session Detection with Optimistic Recording

## Status

Accepted

## Context

Physicians using the transcription app need to manually click "Start Recording" at the beginning of each patient encounter. This creates friction in the clinical workflow and can lead to missed audio if the physician forgets to start recording.

The ideal solution would automatically detect when a patient encounter begins and start recording without physician intervention. This requires:

1. **Detection**: Identify when a clinical encounter has started
2. **Low latency**: Minimize delay between encounter start and recording start
3. **Low false positives**: Avoid starting recordings for non-clinical conversations

The key challenge is that LLM-based greeting detection (using Ollama) takes approximately 35 seconds. If we wait for the greeting check to complete before starting recording, we lose ~35 seconds of conversation audio.

## Decision

Implement auto-session detection using a "listening mode" with **optimistic recording**:

### Detection Pipeline

1. **VAD Monitoring**: When idle and `auto_start_enabled` is true, continuously monitor audio for voice activity
2. **Speech Accumulation**: Wait for 2+ seconds of sustained speech (configurable via `min_speech_duration_ms`)
3. **Whisper Transcription**: Send captured audio to remote Whisper server for transcription
4. **LLM Greeting Check**: Ask Ollama to evaluate if the transcript is a greeting that starts a clinical encounter

### Optimistic Recording Pattern

To prevent losing audio during the ~35s greeting check:

1. **StartRecording Event**: Immediately start recording when sustained speech is detected (before greeting check completes)
2. **Initial Audio Buffer**: Capture the 2+ seconds of speech that triggered the check
3. **Buffer Handoff**: Pass the initial audio buffer to the recording session to prepend to the pipeline
4. **Parallel Check**: Run the greeting check in background while recording continues
5. **GreetingConfirmed**: If greeting detected, recording continues seamlessly
6. **GreetingRejected**: If not a greeting, discard the recording and return to listening

### Event Flow

```
listening_event types:
  - started: Listening mode activated
  - speech_detected: Sustained speech found
  - start_recording: Optimistic recording started (NEW)
  - greeting_confirmed: Check passed, continue recording (NEW)
  - greeting_rejected: Not a greeting, discard recording (NEW)
  - greeting_detected: Legacy event (kept for compatibility)
  - not_greeting: Legacy event (kept for compatibility)
  - error: Error occurred
  - stopped: Listening mode deactivated
```

### Architecture

**Backend** (`listening.rs`):
- Manages audio capture in listening mode (smaller buffer than recording)
- Runs VAD on incoming audio
- Buffers speech audio for transcription
- Calls Whisper server and Ollama for detection
- Emits events to frontend

**Shared State** (`commands/listening.rs`):
- `initial_audio_buffer`: Stores captured audio to prepend to recording
- Consumed by `start_session` and passed to pipeline

**Pipeline Integration** (`pipeline.rs`):
- `PipelineConfig.initial_audio_buffer`: Optional audio to prepend
- At startup, processes initial audio through preprocessing, VAD, and WAV writer

**Frontend** (`useAutoDetection.ts`):
- Manages listening lifecycle
- Provides callbacks: `onStartRecording`, `onGreetingConfirmed`, `onGreetingRejected`
- `isPendingConfirmation`: State for UI feedback during greeting check

### Greeting Detection Prompt

```
Analyze if this is a greeting starting a medical consultation:

TRANSCRIPT: "{transcript}"

Greeting patterns: "Hello", "Hi", "Good morning", "How are you feeling?", etc.

Respond with ONLY JSON:
{"is_greeting": true/false, "confidence": 0.0-1.0, "detected_phrase": "..."}
```

## Consequences

### Positive

- **Zero audio loss**: Optimistic recording ensures no conversation is missed during detection
- **Hands-free operation**: Physicians don't need to manually start recording
- **Seamless UX**: If greeting confirmed, user doesn't notice the detection process
- **Configurable**: Sensitivity and speech duration thresholds can be tuned
- **Fallback**: Manual recording still available if auto-detection fails

### Negative

- **~35s detection latency**: User doesn't know if recording will be kept until check completes
- **False starts**: May start recording conversations that aren't clinical (discarded on rejection)
- **Resource usage**: Listening mode requires constant VAD processing
- **LLM dependency**: Requires Ollama server to be running for detection
- **Complexity**: Optimistic recording pattern adds complexity to state management

### Trade-offs

- **Latency vs Accuracy**: Could use local smaller model for faster detection, but LLM provides better accuracy for varied greeting patterns
- **Buffer Size vs Memory**: Larger initial buffers capture more context but use more memory
- **Cooldown Period**: 5-second cooldown after rejection prevents repeated false triggers

## References

- [Ollama API](https://ollama.ai/docs/api) - LLM inference for greeting detection
- [faster-whisper](https://github.com/SYSTRAN/faster-whisper) - Speech-to-text for transcription
- ADR-0003: VAD-gated processing - Reused VAD infrastructure
- ADR-0009: Ollama SOAP note generation - Reused Ollama client infrastructure
