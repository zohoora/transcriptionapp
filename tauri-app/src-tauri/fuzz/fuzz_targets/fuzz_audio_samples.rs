//! Fuzz testing for audio sample processing
//!
//! Tests that audio processing handles any input samples without panicking.
//!
//! Run with: cargo +nightly fuzz run fuzz_audio_samples

#![no_main]

use libfuzzer_sys::fuzz_target;
use transcription_app::audio::AudioResampler;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to f32 samples
    // Each f32 is 4 bytes, so we need at least 4 bytes
    if data.len() < 4 {
        return;
    }

    // Convert byte slice to f32 samples
    let samples: Vec<f32> = data
        .chunks_exact(4)
        .map(|chunk| {
            let bytes: [u8; 4] = chunk.try_into().unwrap();
            let sample = f32::from_le_bytes(bytes);
            // Clamp to valid audio range and handle NaN/infinity
            if sample.is_nan() || sample.is_infinite() {
                0.0
            } else {
                sample.clamp(-1.0, 1.0)
            }
        })
        .collect();

    if samples.is_empty() {
        return;
    }

    // Test resampler with various sample rates
    for sample_rate in [8000, 16000, 22050, 44100, 48000, 96000] {
        if let Ok(mut resampler) = AudioResampler::new(sample_rate) {
            let input_frames = resampler.input_frames_next();

            // Pad or truncate samples to required size
            let mut input: Vec<f32> = samples.clone();
            input.resize(input_frames, 0.0);

            // Process should not panic
            let _ = resampler.process(&input);
        }
    }
});
