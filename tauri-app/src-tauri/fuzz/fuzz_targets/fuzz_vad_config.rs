//! Fuzz testing for VAD configuration
//!
//! Tests that VadConfig handles any combination of input values without panicking.
//!
//! Run with: cargo +nightly fuzz run fuzz_vad_config

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use transcription_app::vad::{VadConfig, VadGatedPipeline};

/// Arbitrary VAD configuration for fuzzing
#[derive(Debug, Arbitrary)]
struct FuzzVadConfig {
    threshold: f32,
    pre_roll_ms: u32,
    min_speech_ms: u32,
    silence_to_finalize_ms: u32,
    max_utterance_ms: u32,
}

fuzz_target!(|config: FuzzVadConfig| {
    // Clamp threshold to valid range
    let threshold = config.threshold.clamp(0.0, 1.0);

    // Handle NaN and infinity
    let threshold = if threshold.is_nan() { 0.5 } else { threshold };

    // Create config - should never panic
    let vad_config = VadConfig::from_ms(
        threshold,
        config.pre_roll_ms,
        config.min_speech_ms,
        config.silence_to_finalize_ms,
        config.max_utterance_ms,
    );

    // Create pipeline with config - should never panic
    let pipeline = VadGatedPipeline::with_config(vad_config);

    // Basic operations should not panic
    let _ = pipeline.audio_clock_ms();
    let _ = pipeline.is_speech_active();
    let _ = pipeline.has_pending_utterances();
});
