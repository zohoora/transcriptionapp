# ADR-0004: Ring Buffer for Audio Pipeline

## Status

Accepted

## Context

Audio capture runs in a real-time callback from the audio driver. We need to:

- Never block in the audio callback (causes dropouts)
- Buffer enough audio to handle processing variations
- Safely share audio between capture and processing threads

Options considered:
1. **Channels (mpsc)** - Simple but allocates per-sample, GC pressure
2. **Ring buffer** - Lock-free, preallocated, efficient
3. **Shared Vec with Mutex** - Simple but blocking, not real-time safe

## Decision

We chose a **lock-free ring buffer** using the `ringbuf` crate.

The pipeline:
```
Audio Callback -> Ring Buffer Producer -> Ring Buffer Consumer -> Processing Thread
```

Ring buffer sized for 30 seconds of audio at device sample rate, allowing for:
- Temporary processing delays
- Whisper inference latency
- System scheduling variations

## Consequences

### Positive

- Zero allocation in audio callback
- Lock-free for real-time safety
- Predictable memory usage
- Handles sample rate variations

### Negative

- Fixed buffer size (potential overflow in extreme cases)
- More complex than simple channels
- Requires careful capacity calculation

## References

- [ringbuf crate](https://crates.io/crates/ringbuf)
- [Real-Time Audio Programming Guide](https://www.rossbencina.com/code/real-time-audio-programming-101)
