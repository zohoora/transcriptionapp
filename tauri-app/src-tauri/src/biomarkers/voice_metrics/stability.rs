//! Stability metric: CPP (Cepstral Peak Prominence)
//!
//! ## Concept
//! Measures vocal fold regularity to detect fatigue or tremors (Parkinson's).
//! Higher CPP = more stable/regular voice.
//!
//! ## Why CPP instead of Jitter/Shimmer?
//! Jitter and Shimmer fail in ambient noise. CPP is more robust because it
//! operates in the cepstral domain, which is inherently noise-resistant.
//!
//! ## Algorithm
//! 1. Apply Hanning window to reduce spectral leakage
//! 2. Perform FFT on the windowed signal
//! 3. Calculate log magnitude of the spectrum
//! 4. Perform IFFT to get the real cepstrum
//! 5. Find the highest peak in the "quefrency" range (2ms-20ms = 50Hz-500Hz pitch)
//! 6. Return the amplitude of this peak relative to average cepstral energy (in dB)

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::f32::consts::PI;

/// Minimum quefrency in ms (corresponds to 500Hz max pitch)
const QUEFRENCY_MIN_MS: f32 = 2.0;

/// Maximum quefrency in ms (corresponds to 50Hz min pitch)
const QUEFRENCY_MAX_MS: f32 = 20.0;

/// Minimum samples required for CPP calculation
const MIN_SAMPLES: usize = 512;

/// Calculate stability (CPP - Cepstral Peak Prominence) from audio samples.
///
/// Returns CPP in dB. Higher values indicate more stable/regular voice.
/// Typical values: 3-10 dB for healthy voices.
///
/// Returns `None` if the audio is too short or no clear pitch detected.
pub fn calculate_stability(samples: &[f32], sample_rate: usize) -> Option<f32> {
    if samples.len() < MIN_SAMPLES {
        return None;
    }

    // Pad to next power of 2 for efficient FFT
    let n = samples.len().next_power_of_two();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);

    // Apply Hanning window to reduce spectral leakage
    let windowed: Vec<f32> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let window = 0.5 - 0.5 * (2.0 * PI * i as f32 / samples.len() as f32).cos();
            s * window
        })
        .collect();

    // Prepare FFT input (zero-padded)
    let mut spectrum: Vec<Complex<f32>> = windowed
        .iter()
        .map(|&s| Complex::new(s, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)).take(n - samples.len()))
        .collect();

    // Forward FFT
    fft.process(&mut spectrum);

    // Log magnitude spectrum (with small epsilon to avoid log(0))
    let log_mag: Vec<Complex<f32>> = spectrum
        .iter()
        .map(|c| Complex::new((c.norm() + 1e-10).ln(), 0.0))
        .collect();

    // IFFT to get real cepstrum
    let mut cepstrum = log_mag;
    ifft.process(&mut cepstrum);

    // Normalize by FFT size
    let scale = 1.0 / n as f32;
    for c in &mut cepstrum {
        c.re *= scale;
        c.im *= scale;
    }

    // Find peak in quefrency range corresponding to human pitch
    // quefrency (in samples) = sample_rate / frequency
    // So for 50Hz: quefrency = 16000/50 = 320 samples (20ms)
    // For 500Hz: quefrency = 16000/500 = 32 samples (2ms)
    let min_idx = (QUEFRENCY_MIN_MS * sample_rate as f32 / 1000.0) as usize;
    let max_idx = ((QUEFRENCY_MAX_MS * sample_rate as f32 / 1000.0) as usize).min(n / 2);

    if min_idx >= max_idx || max_idx >= cepstrum.len() {
        return None;
    }

    // Find peak value in quefrency range
    let mut peak_value = 0.0f32;
    let mut sum = 0.0f32;

    for i in min_idx..max_idx {
        let value = cepstrum[i].re.abs();
        if value > peak_value {
            peak_value = value;
        }
        sum += value;
    }

    let avg_energy = sum / (max_idx - min_idx) as f32;

    // Avoid division by zero
    if avg_energy < 1e-10 {
        return None;
    }

    // CPP in dB: how much the peak stands out from the average
    let cpp = 20.0 * (peak_value / avg_energy).log10();

    // Sanity check - CPP should be positive and reasonable
    if cpp.is_nan() || cpp < 0.0 || cpp > 50.0 {
        return None;
    }

    Some(cpp)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Generate noise
    fn generate_noise(sample_rate: usize, duration_ms: u32) -> Vec<f32> {
        let num_samples = (sample_rate as u32 * duration_ms / 1000) as usize;
        // Simple pseudo-random noise using linear congruential generator
        let mut seed = 12345u32;
        (0..num_samples)
            .map(|_| {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                ((seed >> 16) as f32 / 32768.0 - 1.0) * 0.3
            })
            .collect()
    }

    #[test]
    fn test_calculate_stability_pure_tone() {
        // A pure tone should have high CPP (very stable)
        let samples = generate_sine(200.0, 16000, 500);
        let result = calculate_stability(&samples, 16000);

        assert!(result.is_some());
        let cpp = result.unwrap();

        // Pure tone should have high CPP
        assert!(cpp > 5.0, "Expected high CPP for pure tone, got {}", cpp);
    }

    #[test]
    fn test_calculate_stability_noise() {
        // Noise should have lower CPP
        let samples = generate_noise(16000, 500);
        let result = calculate_stability(&samples, 16000);

        // Noise might not have a clear peak at all
        if let Some(cpp) = result {
            // If we get a result, it should be lower than pure tone
            assert!(cpp < 15.0, "Expected lower CPP for noise, got {}", cpp);
        }
    }

    #[test]
    fn test_calculate_stability_insufficient_samples() {
        let samples = vec![0.0; 100]; // Too short
        let result = calculate_stability(&samples, 16000);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_stability_with_harmonics() {
        // Voice has harmonics - create fundamental + harmonics
        let sample_rate = 16000;
        let duration_ms = 500;
        let num_samples = (sample_rate as u32 * duration_ms / 1000) as usize;
        let fundamental = 150.0;

        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                // Fundamental + 2nd + 3rd harmonic (typical voice)
                (2.0 * PI * fundamental * t).sin() * 0.5
                    + (2.0 * PI * fundamental * 2.0 * t).sin() * 0.25
                    + (2.0 * PI * fundamental * 3.0 * t).sin() * 0.125
            })
            .collect();

        let result = calculate_stability(&samples, 16000);
        assert!(result.is_some());

        let cpp = result.unwrap();
        // Harmonic signal should have good CPP
        assert!(cpp > 3.0, "Expected reasonable CPP for harmonic signal, got {}", cpp);
    }
}
