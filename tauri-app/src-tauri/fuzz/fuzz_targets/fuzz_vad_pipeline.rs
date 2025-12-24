//! Fuzz testing for VAD pipeline operations
//!
//! Tests that the VAD pipeline handles any sequence of operations without panicking.
//!
//! Run with: cargo +nightly fuzz run fuzz_vad_pipeline

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use transcription_app::vad::{VadConfig, VadGatedPipeline};

/// Operations that can be performed on the pipeline
#[derive(Debug, Arbitrary)]
enum PipelineOp {
    /// Advance the audio clock by some samples
    AdvanceClock { samples: u16 },
    /// Force flush any pending utterance
    ForceFlush,
    /// Pop an utterance if available
    PopUtterance,
    /// Check various state flags
    CheckState,
}

/// Fuzzing input: a sequence of operations
#[derive(Debug, Arbitrary)]
struct FuzzInput {
    /// Initial configuration values
    threshold: f32,
    pre_roll_ms: u16,
    min_speech_ms: u16,
    silence_ms: u16,
    max_utterance_ms: u16,
    /// Sequence of operations to perform
    operations: Vec<PipelineOp>,
}

fuzz_target!(|input: FuzzInput| {
    // Limit operations to prevent timeout
    if input.operations.len() > 1000 {
        return;
    }

    // Create config with sanitized values
    let threshold = input.threshold.clamp(0.0, 1.0);
    let threshold = if threshold.is_nan() { 0.5 } else { threshold };

    let config = VadConfig::from_ms(
        threshold,
        input.pre_roll_ms as u32,
        input.min_speech_ms as u32,
        input.silence_ms as u32,
        input.max_utterance_ms as u32,
    );

    let mut pipeline = VadGatedPipeline::with_config(config);

    // Execute operations
    for op in input.operations {
        match op {
            PipelineOp::AdvanceClock { samples } => {
                pipeline.advance_audio_clock(samples as usize);
            }
            PipelineOp::ForceFlush => {
                pipeline.force_flush();
            }
            PipelineOp::PopUtterance => {
                let _ = pipeline.pop_utterance();
            }
            PipelineOp::CheckState => {
                let _ = pipeline.audio_clock_ms();
                let _ = pipeline.is_speech_active();
                let _ = pipeline.has_pending_utterances();
            }
        }
    }

    // Final state checks should not panic
    let _ = pipeline.audio_clock_ms();
    let _ = pipeline.is_speech_active();
    pipeline.force_flush();
    while pipeline.has_pending_utterances() {
        let _ = pipeline.pop_utterance();
    }
});
