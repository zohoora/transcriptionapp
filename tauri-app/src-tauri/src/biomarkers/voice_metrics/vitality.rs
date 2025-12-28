//! Vitality metric: Pitch variability via F0 standard deviation
//!
//! ## Concept
//! Measures pitch variability to detect "flat affect" (Depression/PTSD).
//! Higher vitality = more pitch variation = more emotional engagement.
//!
//! ## Algorithm
//! 1. Segment audio into small frames (30-50ms)
//! 2. Use McLeod algorithm to find F0 (Fundamental Frequency)
//! 3. Filter results to human vocal range (50Hz - 500Hz)
//! 4. Calculate standard deviation of valid pitch values
//! 5. Return 0.0 if insufficient voiced frames detected

use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;

/// Frame size for pitch detection (~64ms at 16kHz)
const FRAME_SIZE: usize = 1024;

/// Hop size between frames (50% overlap)
const HOP_SIZE: usize = 512;

/// Minimum pitch in Hz (human vocal range)
const MIN_PITCH: f32 = 50.0;

/// Maximum pitch in Hz (human vocal range)
const MAX_PITCH: f32 = 500.0;

/// Power threshold for pitch detection
const POWER_THRESHOLD: f32 = 0.8;

/// Clarity threshold for pitch detection
const CLARITY_THRESHOLD: f32 = 0.5;

/// Minimum number of voiced frames required for valid vitality
const MIN_VOICED_FRAMES: usize = 5;

/// Calculate vitality (pitch variability) from audio samples.
///
/// Returns `Some((vitality, f0_mean, voiced_ratio))` if enough voiced frames detected.
/// - `vitality`: Standard deviation of F0 in Hz
/// - `f0_mean`: Mean F0 in Hz
/// - `voiced_ratio`: Fraction of frames with valid pitch (0.0-1.0)
///
/// Returns `None` if insufficient voiced frames detected.
pub fn calculate_vitality(samples: &[f32], sample_rate: usize) -> Option<(f32, f32, f32)> {
    if samples.len() < FRAME_SIZE {
        return None;
    }

    let mut detector = McLeodDetector::new(FRAME_SIZE, FRAME_SIZE / 2);
    let mut pitches = Vec::new();
    let mut total_frames = 0;

    // Process frames with hop
    let mut start = 0;
    while start + FRAME_SIZE <= samples.len() {
        let frame = &samples[start..start + FRAME_SIZE];
        total_frames += 1;

        // Detect pitch
        if let Some(pitch) = detector.get_pitch(frame, sample_rate, POWER_THRESHOLD, CLARITY_THRESHOLD) {
            // Filter to human vocal range
            if pitch.frequency >= MIN_PITCH && pitch.frequency <= MAX_PITCH {
                pitches.push(pitch.frequency);
            }
        }

        start += HOP_SIZE;
    }

    // Need sufficient voiced frames
    if pitches.len() < MIN_VOICED_FRAMES {
        return None;
    }

    // Calculate mean
    let mean = pitches.iter().sum::<f32>() / pitches.len() as f32;

    // Calculate variance
    let variance = pitches.iter().map(|p| (p - mean).powi(2)).sum::<f32>() / pitches.len() as f32;

    // Standard deviation is vitality
    let vitality = variance.sqrt();

    // Voiced ratio
    let voiced_ratio = pitches.len() as f32 / total_frames as f32;

    Some((vitality, mean, voiced_ratio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    /// Generate a sine wave at a given frequency
    fn generate_sine(freq: f32, sample_rate: usize, duration_ms: u32) -> Vec<f32> {
        let num_samples = (sample_rate as u32 * duration_ms / 1000) as usize;
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * PI * freq * t).sin() * 0.5
            })
            .collect()
    }

    #[test]
    fn test_calculate_vitality_constant_pitch() {
        // A constant pitch should have very low vitality (near 0 std dev)
        let samples = generate_sine(200.0, 16000, 1000); // 200 Hz for 1 second
        let result = calculate_vitality(&samples, 16000);

        assert!(result.is_some());
        let (vitality, f0_mean, voiced_ratio) = result.unwrap();

        // Constant pitch should have very low std dev
        assert!(vitality < 10.0, "Expected low vitality, got {}", vitality);
        // Mean should be near 200 Hz
        assert!((f0_mean - 200.0).abs() < 20.0, "Expected mean ~200 Hz, got {}", f0_mean);
        // Should have high voiced ratio
        assert!(voiced_ratio > 0.5);
    }

    #[test]
    fn test_calculate_vitality_insufficient_samples() {
        let samples = vec![0.0; 100]; // Too short
        let result = calculate_vitality(&samples, 16000);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_vitality_silence() {
        // Silence should have no voiced frames
        let samples = vec![0.0; 16000]; // 1 second of silence
        let result = calculate_vitality(&samples, 16000);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_vitality_varying_pitch() {
        // Create audio with varying pitch - should have higher vitality
        let mut samples = Vec::new();

        // First half at 150 Hz
        samples.extend(generate_sine(150.0, 16000, 500));
        // Second half at 250 Hz
        samples.extend(generate_sine(250.0, 16000, 500));

        let result = calculate_vitality(&samples, 16000);
        assert!(result.is_some());

        let (vitality, f0_mean, _) = result.unwrap();

        // Should have higher vitality due to pitch variation
        assert!(vitality > 10.0, "Expected higher vitality for varying pitch, got {}", vitality);
        // Mean should be between 150 and 250
        assert!(f0_mean > 100.0 && f0_mean < 300.0);
    }
}
