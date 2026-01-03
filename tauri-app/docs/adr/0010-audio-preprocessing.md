# ADR-0010: Audio Preprocessing Pipeline

## Status

Accepted

## Context

The transcription pipeline receives raw audio from the microphone, but several issues can degrade ASR quality:

1. **DC offset** - Many microphones have a slight DC bias that wastes dynamic range
2. **Low-frequency noise** - 50/60Hz power hum, HVAC rumble, desk vibrations
3. **Inconsistent levels** - Speakers at varying distances produce different volume levels

Research shows that Whisper is trained on noisy audio, so traditional noise reduction can actually *hurt* performance. However, level normalization and low-frequency filtering significantly *help*.

## Decision

We implemented a three-stage preprocessing pipeline that runs after resampling but before VAD:

```
Resampler (16kHz) → DC Removal → High-Pass Filter → AGC → VAD → Enhancement → Whisper
```

### Stage 1: DC Offset Removal

Single-pole IIR filter to remove microphone DC bias:
```rust
struct DcBlocker {
    prev_input: f32,
    prev_output: f32,
    alpha: f32,  // 0.995 default
}
```

- Time constant: ~200ms at 16kHz
- Minimal impact on speech frequencies
- Prevents issues with downstream filters

### Stage 2: High-Pass Filter (80Hz)

2nd-order Butterworth biquad filter using `biquad` crate:

```rust
let coeffs = Coefficients::<f32>::from_params(
    Type::HighPass,
    sample_rate.hz(),
    cutoff_hz.hz(),
    Q_BUTTERWORTH_F32,
)?;
let filter = DirectForm2Transposed::<f32>::new(coeffs);
```

- Removes 50/60Hz power line hum
- Removes HVAC and ventilation rumble
- Removes desk vibrations and footsteps
- These frequencies contain no speech information

### Stage 3: Automatic Gain Control

Digital AGC using `dagc` crate to normalize audio levels:

```rust
let agc = MonoAgc::new(target_rms, distortion);
agc.process(samples);
```

- Target RMS: 0.1 (~-20dBFS)
- Handles varying speaker distances
- Consistent input level improves Whisper accuracy
- Clinical settings have varying room acoustics

## Alternatives Considered

### Noise Reduction (RNNoise, NSX, etc.)
**Rejected**: Research shows Whisper performs *worse* with aggressive denoising because it's trained on noisy audio. We already have optional GTCRN enhancement for cases that need it.

### VAD-gated Preprocessing
**Rejected**: AGC needs continuous input to maintain gain state. Applying only during speech creates inconsistent levels.

### Pre-emphasis Filter
**Rejected**: Uncertain benefit with modern ASR. Could interfere with enhancement model. May reconsider in future.

### GPU-based Preprocessing
**Rejected**: Overkill for simple IIR filtering and AGC. CPU is sufficient and avoids GPU context switching.

## Consequences

### Positive

- Improved transcription accuracy in clinical environments
- Handles 50/60Hz hum from medical equipment
- Consistent levels regardless of speaker distance
- Minimal latency (<0.5ms total)
- Negligible CPU overhead
- No external dependencies or models required
- Configurable (can be disabled if needed)

### Negative

- Two new crate dependencies (`dagc`, `biquad`)
- Additional configuration options for users
- May require tuning for specific environments

## Configuration

```rust
pub preprocessing_enabled: bool,         // default: true
pub preprocessing_highpass_hz: u32,      // default: 80
pub preprocessing_agc_target_rms: f32,   // default: 0.1
```

## Performance

| Stage | Latency | CPU Cost |
|-------|---------|----------|
| DC Offset | ~0 | O(n) trivial |
| High-Pass | ~0.1ms | O(n) IIR biquad |
| AGC | ~0.1ms | O(n) with state |
| **Total** | <0.5ms | Negligible |

## References

- [Whisper Training Data](https://github.com/openai/whisper) - Trained on noisy web audio
- [EBU R128 Loudness Standard](https://tech.ebu.ch/docs/r/r128.pdf) - Target level guidance
- [biquad crate](https://crates.io/crates/biquad) - IIR filter implementation
- [dagc crate](https://crates.io/crates/dagc) - Digital AGC implementation
