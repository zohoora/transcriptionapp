//! # Audio Preprocessing Module
//!
//! This module provides audio preprocessing for improved transcription quality.
//! It applies the following processing chain in order:
//!
//! 1. **DC Offset Removal** - Removes DC bias from microphone input
//! 2. **High-Pass Filter** - Removes low-frequency rumble and hum (< 80Hz)
//! 3. **AGC (Automatic Gain Control)** - Normalizes audio levels
//!
//! ## Why These Steps?
//!
//! - **DC Offset**: Many microphones have a slight DC bias that wastes dynamic range
//! - **High-Pass**: Removes 50/60Hz power hum, HVAC rumble, desk vibrations
//! - **AGC**: Normalizes varying speaker distances and volumes for consistent Whisper input
//!
//! ## Note on Noise Reduction
//!
//! Traditional noise reduction (NSX, RNNoise) is NOT included because Whisper
//! is trained on noisy audio and performs worse with aggressive denoising.
//! The optional GTCRN enhancement module handles this separately if needed.

use anyhow::Result;
use biquad::{Biquad, Coefficients, DirectForm2Transposed, ToHertz, Type, Q_BUTTERWORTH_F32};
use dagc::MonoAgc;
use tracing::{debug, info};

/// Audio preprocessor that applies DC removal, high-pass filtering, and AGC.
///
/// This struct is designed to process audio in chunks as they arrive from
/// the resampler, maintaining state between calls for continuous operation.
pub struct AudioPreprocessor {
    /// DC blocking filter state
    dc_blocker: DcBlocker,
    /// High-pass filter (removes rumble/hum below cutoff)
    highpass: DirectForm2Transposed<f32>,
    /// Automatic gain control
    agc: MonoAgc,
    /// High-pass cutoff frequency in Hz
    highpass_cutoff_hz: u32,
    /// Whether preprocessing is enabled
    enabled: bool,
}

/// Simple DC blocking filter using single-pole IIR.
///
/// Implements: y[n] = x[n] - x[n-1] + alpha * y[n-1]
/// where alpha is typically ~0.995 for audio.
struct DcBlocker {
    /// Filter coefficient (typically 0.995 for smooth DC removal)
    alpha: f32,
    /// Previous input sample
    x_prev: f32,
    /// Previous output sample
    y_prev: f32,
}

impl DcBlocker {
    /// Create a new DC blocker with the given time constant.
    ///
    /// # Arguments
    /// * `sample_rate` - Sample rate in Hz
    /// * `cutoff_hz` - Approximate cutoff frequency (typically 5-20 Hz)
    fn new(sample_rate: u32, cutoff_hz: f32) -> Self {
        // Calculate alpha from cutoff frequency
        // alpha ≈ 1 - (2 * pi * fc / fs)
        let alpha = 1.0 - (2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32);
        let alpha = alpha.clamp(0.9, 0.9999); // Ensure stability

        Self {
            alpha,
            x_prev: 0.0,
            y_prev: 0.0,
        }
    }

    /// Process a single sample through the DC blocker.
    #[inline]
    fn process_sample(&mut self, x: f32) -> f32 {
        let y = x - self.x_prev + self.alpha * self.y_prev;
        self.x_prev = x;
        self.y_prev = y;
        y
    }

    /// Process a buffer of samples in-place.
    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            *sample = self.process_sample(*sample);
        }
    }

    /// Reset the filter state.
    fn reset(&mut self) {
        self.x_prev = 0.0;
        self.y_prev = 0.0;
    }
}

impl AudioPreprocessor {
    /// Default high-pass cutoff frequency in Hz.
    pub const DEFAULT_HIGHPASS_HZ: u32 = 80;

    /// Default AGC target RMS level (linear scale, approximately -20 dBFS).
    /// 0.1 corresponds to about -20 dBFS.
    pub const DEFAULT_AGC_TARGET_RMS: f32 = 0.1;

    /// Default AGC distortion factor (controls adaptation speed).
    /// Lower values = slower, smoother adaptation. 0.001 is a good starting point.
    pub const DEFAULT_AGC_DISTORTION: f32 = 0.001;

    /// Create a new audio preprocessor.
    ///
    /// # Arguments
    /// * `sample_rate` - Sample rate in Hz (typically 16000 for Whisper)
    /// * `highpass_cutoff_hz` - High-pass filter cutoff frequency (default: 80 Hz)
    /// * `agc_target_rms` - Target RMS level for AGC (default: 0.1, ~-20 dBFS)
    ///
    /// # Errors
    /// Returns an error if filter coefficients cannot be calculated.
    pub fn new(sample_rate: u32, highpass_cutoff_hz: u32, agc_target_rms: f32) -> Result<Self> {
        info!(
            "Creating audio preprocessor: {}Hz sample rate, {}Hz highpass, {:.3} target RMS",
            sample_rate, highpass_cutoff_hz, agc_target_rms
        );

        // Create DC blocker with ~10Hz cutoff
        let dc_blocker = DcBlocker::new(sample_rate, 10.0);

        // Create high-pass filter coefficients
        let highpass_coeffs = Coefficients::<f32>::from_params(
            Type::HighPass,
            sample_rate.hz(),
            highpass_cutoff_hz.hz(),
            Q_BUTTERWORTH_F32,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create high-pass filter coefficients: {:?}", e))?;

        let highpass = DirectForm2Transposed::<f32>::new(highpass_coeffs);

        // Create AGC
        let agc = MonoAgc::new(agc_target_rms, Self::DEFAULT_AGC_DISTORTION)
            .map_err(|e| anyhow::anyhow!("Failed to create AGC: {:?}", e))?;

        debug!(
            "Preprocessor initialized: DC blocker alpha={:.4}, highpass={}Hz, AGC target={}",
            dc_blocker.alpha, highpass_cutoff_hz, agc_target_rms
        );

        Ok(Self {
            dc_blocker,
            highpass,
            agc,
            highpass_cutoff_hz,
            enabled: true,
        })
    }

    /// Create a new preprocessor with default settings for 16kHz audio.
    pub fn new_default() -> Result<Self> {
        Self::new(16000, Self::DEFAULT_HIGHPASS_HZ, Self::DEFAULT_AGC_TARGET_RMS)
    }

    /// Enable or disable preprocessing.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if preprocessing is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Freeze/unfreeze AGC gain updates.
    ///
    /// Useful during non-speech segments to prevent amplifying background noise.
    pub fn freeze_gain(&mut self, freeze: bool) {
        self.agc.freeze_gain(freeze);
    }

    /// Get the current AGC gain value.
    pub fn current_gain(&self) -> f32 {
        self.agc.gain()
    }

    /// Get the high-pass cutoff frequency.
    pub fn highpass_cutoff_hz(&self) -> u32 {
        self.highpass_cutoff_hz
    }

    /// Process a buffer of audio samples in-place.
    ///
    /// Applies the full preprocessing chain:
    /// 1. DC offset removal
    /// 2. High-pass filtering
    /// 3. Automatic gain control
    ///
    /// If preprocessing is disabled, this is a no-op.
    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled || samples.is_empty() {
            return;
        }

        // Step 1: Remove DC offset
        self.dc_blocker.process(samples);

        // Step 2: Apply high-pass filter
        for sample in samples.iter_mut() {
            *sample = self.highpass.run(*sample);
        }

        // Step 3: Apply AGC
        self.agc.process(samples);
    }

    /// Reset all filter states.
    ///
    /// Call this when starting a new recording session.
    pub fn reset(&mut self) {
        self.dc_blocker.reset();
        // Note: DirectForm2Transposed doesn't have a public reset method,
        // but the filter will naturally converge within a few samples.
        // If needed, we could recreate it, but for audio this is fine.
        debug!("Preprocessor state reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Helper to calculate RMS of a buffer
    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|x| x * x).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    // Helper to calculate DC offset (mean) of a buffer
    fn calculate_dc_offset(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        samples.iter().sum::<f32>() / samples.len() as f32
    }

    // Helper to generate a sine wave
    fn generate_sine(freq_hz: f32, sample_rate: u32, duration_samples: usize, amplitude: f32) -> Vec<f32> {
        (0..duration_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                amplitude * (2.0 * std::f32::consts::PI * freq_hz * t).sin()
            })
            .collect()
    }

    #[test]
    fn test_preprocessor_creation() {
        let result = AudioPreprocessor::new(16000, 80, 0.1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_preprocessor_default() {
        let result = AudioPreprocessor::new_default();
        assert!(result.is_ok());
        let pp = result.unwrap();
        assert_eq!(pp.highpass_cutoff_hz(), 80);
    }

    #[test]
    fn test_dc_blocker_removes_offset() {
        let mut blocker = DcBlocker::new(16000, 10.0);

        // Create signal with DC offset
        let dc_offset = 0.3;
        let mut samples: Vec<f32> = (0..1600)
            .map(|i| {
                let t = i as f32 / 16000.0;
                dc_offset + 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();

        // Verify initial DC offset
        let initial_dc = calculate_dc_offset(&samples);
        assert!((initial_dc - dc_offset).abs() < 0.1);

        blocker.process(&mut samples);

        // After DC blocking, mean should be near zero
        // (allow some settling time - check last half)
        let final_dc = calculate_dc_offset(&samples[800..]);
        assert!(
            final_dc.abs() < 0.05,
            "DC offset should be removed, got {}",
            final_dc
        );
    }

    #[test]
    fn test_highpass_attenuates_low_frequencies() {
        let mut pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();

        // Generate a 50Hz sine wave (should be attenuated)
        let mut low_freq = generate_sine(50.0, 16000, 3200, 0.5);

        // Generate a 500Hz sine wave (should pass through)
        let mut high_freq = generate_sine(500.0, 16000, 3200, 0.5);

        let low_rms_before = calculate_rms(&low_freq);
        let high_rms_before = calculate_rms(&high_freq);

        // Disable AGC to isolate filter effect
        pp.agc.freeze_gain(true);

        pp.process(&mut low_freq);
        pp.process(&mut high_freq);

        let low_rms_after = calculate_rms(&low_freq[1600..]); // Skip transient
        let high_rms_after = calculate_rms(&high_freq[1600..]);

        // Low frequency should be significantly attenuated
        let low_attenuation = low_rms_after / low_rms_before;
        assert!(
            low_attenuation < 0.5,
            "50Hz should be attenuated, ratio: {}",
            low_attenuation
        );

        // High frequency should largely pass through
        let high_attenuation = high_rms_after / high_rms_before;
        assert!(
            high_attenuation > 0.5,
            "500Hz should pass through, ratio: {}",
            high_attenuation
        );
    }

    #[test]
    fn test_agc_normalizes_quiet_signal() {
        let mut pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();

        // Generate a quiet signal (0.01 RMS, much below target 0.1)
        let mut quiet_signal = generate_sine(440.0, 16000, 16000, 0.01);
        let rms_before = calculate_rms(&quiet_signal);

        // Process multiple times to let AGC converge
        for chunk in quiet_signal.chunks_mut(1600) {
            pp.process(chunk);
        }

        let rms_after = calculate_rms(&quiet_signal[8000..]);

        // AGC should bring the signal closer to target
        assert!(
            rms_after > rms_before,
            "AGC should amplify quiet signal: before={}, after={}",
            rms_before,
            rms_after
        );
    }

    #[test]
    fn test_agc_normalizes_loud_signal() {
        let mut pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();

        // Generate a loud signal (0.5 RMS, above target 0.1)
        let mut loud_signal = generate_sine(440.0, 16000, 16000, 0.5);
        let rms_before = calculate_rms(&loud_signal);

        // Process multiple times to let AGC converge
        for chunk in loud_signal.chunks_mut(1600) {
            pp.process(chunk);
        }

        let rms_after = calculate_rms(&loud_signal[8000..]);

        // AGC should bring the signal closer to target
        assert!(
            rms_after < rms_before,
            "AGC should attenuate loud signal: before={}, after={}",
            rms_before,
            rms_after
        );
    }

    #[test]
    fn test_preprocessor_disabled() {
        let mut pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();
        pp.set_enabled(false);

        let original = vec![0.1, 0.2, 0.3, -0.1, -0.2];
        let mut samples = original.clone();

        pp.process(&mut samples);

        // Samples should be unchanged when disabled
        assert_eq!(samples, original);
    }

    #[test]
    fn test_preprocessor_empty_input() {
        let mut pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();
        let mut empty: Vec<f32> = vec![];
        pp.process(&mut empty); // Should not panic
    }

    #[test]
    fn test_freeze_gain() {
        let mut pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();

        assert!(!pp.agc.is_gain_frozen());
        pp.freeze_gain(true);
        assert!(pp.agc.is_gain_frozen());
        pp.freeze_gain(false);
        assert!(!pp.agc.is_gain_frozen());
    }

    #[test]
    fn test_current_gain() {
        let pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();
        let gain = pp.current_gain();
        assert!(gain > 0.0, "Initial gain should be positive");
    }

    proptest! {
        #[test]
        fn prop_dc_blocker_produces_finite_output(
            samples in proptest::collection::vec(-1.0f32..1.0, 1..1000)
        ) {
            let mut blocker = DcBlocker::new(16000, 10.0);
            let mut output = samples.clone();
            blocker.process(&mut output);

            for sample in &output {
                prop_assert!(sample.is_finite());
                prop_assert!(*sample >= -10.0 && *sample <= 10.0);
            }
        }

        #[test]
        fn prop_preprocessor_produces_finite_output(
            samples in proptest::collection::vec(-1.0f32..1.0, 100..1000)
        ) {
            let mut pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();
            let mut output = samples.clone();
            pp.process(&mut output);

            for sample in &output {
                prop_assert!(sample.is_finite());
                // AGC might amplify, but should stay reasonable
                prop_assert!(sample.abs() < 100.0);
            }
        }

        #[test]
        fn prop_different_sample_rates_work(
            sample_rate in 8000u32..96000
        ) {
            // Should not panic or error for valid sample rates
            let result = AudioPreprocessor::new(sample_rate, 80, 0.1);
            // Some sample rates might fail if cutoff > nyquist
            if sample_rate > 160 {
                prop_assert!(result.is_ok());
            }
        }

        #[test]
        fn prop_highpass_preserves_signal_energy_above_cutoff(
            freq in 200.0f32..4000.0,
            amplitude in 0.1f32..0.9
        ) {
            let mut pp = AudioPreprocessor::new(16000, 80, 0.1).unwrap();
            pp.agc.freeze_gain(true); // Disable AGC for this test

            let mut signal = generate_sine(freq, 16000, 3200, amplitude);
            let rms_before = calculate_rms(&signal);

            pp.process(&mut signal);

            let rms_after = calculate_rms(&signal[1600..]);

            // Signal above cutoff should retain at least 50% of energy
            let ratio = rms_after / rms_before;
            prop_assert!(
                ratio > 0.5,
                "Frequency {}Hz should pass through, ratio: {}",
                freq,
                ratio
            );
        }
    }
}
