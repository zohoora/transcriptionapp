//! Fuzz testing for Segment and Utterance types
//!
//! Tests that transcription types handle any input without panicking.
//!
//! Run with: cargo +nightly fuzz run fuzz_segment

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use transcription_app::transcription::{Segment, Utterance};

/// Arbitrary segment data for fuzzing
#[derive(Debug, Arbitrary)]
struct FuzzSegment {
    start_ms: u64,
    end_ms: u64,
    text: String,
}

/// Arbitrary utterance data for fuzzing
#[derive(Debug, Arbitrary)]
struct FuzzUtterance {
    audio_len: usize,
    start_ms: u64,
    end_ms: u64,
}

fuzz_target!(|data: (FuzzSegment, FuzzUtterance)| {
    let (seg_data, utt_data) = data;

    // Test Segment creation with any values
    let segment = Segment::new(seg_data.start_ms, seg_data.end_ms, seg_data.text.clone());

    // Verify fields are set correctly
    assert_eq!(segment.start_ms, seg_data.start_ms);
    assert_eq!(segment.end_ms, seg_data.end_ms);
    assert_eq!(segment.text, seg_data.text);

    // Duration should handle underflow
    let _ = segment.duration_ms();

    // Test Utterance creation
    // Limit audio size to prevent OOM
    let audio_len = utt_data.audio_len.min(16000 * 60); // Max 1 minute at 16kHz
    let audio: Vec<f32> = (0..audio_len)
        .map(|i| ((i as f32) * 0.001).sin())
        .collect();

    let utterance = Utterance::new(audio.clone(), utt_data.start_ms, utt_data.end_ms);

    // Verify fields
    assert_eq!(utterance.audio.len(), audio_len);
    assert_eq!(utterance.start_ms, utt_data.start_ms);
    assert_eq!(utterance.end_ms, utt_data.end_ms);

    // Duration should handle underflow
    let _ = utterance.duration_ms();
});
