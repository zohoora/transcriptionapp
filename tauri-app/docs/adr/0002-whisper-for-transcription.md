# ADR-0002: Use Whisper for Speech Transcription

## Status

Accepted

## Context

We need a speech-to-text engine that can:

- Run locally without network dependency
- Provide high-quality transcription
- Support multiple languages
- Be fast enough for near-real-time use

Options considered:
1. **Cloud APIs** (Google, AWS, Azure) - High quality but requires internet, privacy concerns
2. **Whisper (OpenAI)** - Open source, runs locally, multiple model sizes
3. **Vosk** - Lightweight, fast, but lower accuracy
4. **DeepSpeech** - Mozilla project, discontinued

## Decision

We chose **OpenAI Whisper** via the whisper-rs Rust bindings.

Whisper provides:
- State-of-the-art transcription accuracy
- Multiple model sizes (tiny to large) for speed/accuracy trade-offs
- Multilingual support (99 languages)
- Local processing for privacy
- Active development and community

## Consequences

### Positive

- Excellent transcription quality
- No cloud dependency or API costs
- User data stays local
- Model size options allow performance tuning
- Works offline

### Negative

- Larger model files (75MB - 3GB depending on model)
- Higher CPU/GPU requirements than cloud APIs
- Real-time processing requires VAD chunking strategy
- Initial model download required

## References

- [Whisper Paper](https://arxiv.org/abs/2212.04356)
- [whisper-rs](https://github.com/tazz4843/whisper-rs)
- [GGML Whisper Models](https://huggingface.co/ggerganov/whisper.cpp)
