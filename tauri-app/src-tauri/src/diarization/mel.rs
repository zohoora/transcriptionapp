//! Mel spectrogram generation for speaker embeddings.
//!
//! Converts raw audio waveforms to 80-band log mel spectrograms
//! as expected by the ECAPA-TDNN speaker embedding model.

#[cfg(feature = "diarization")]
use realfft::{RealFftPlanner, RealToComplex};

use super::config::MelConfig;
use super::DiarizationError;
use std::f32::consts::PI;

#[cfg(feature = "diarization")]
use std::sync::Arc;

/// Mel spectrogram generator with pre-computed filterbank and FFT plan
#[cfg(feature = "diarization")]
pub struct MelSpectrogramGenerator {
    config: MelConfig,
    fft: Arc<dyn RealToComplex<f32>>,
    mel_filterbank: Vec<Vec<f32>>,
    window: Vec<f32>,
    // Pre-allocated buffers
    fft_input: Vec<f32>,
    fft_output: Vec<realfft::num_complex::Complex<f32>>,
}

#[cfg(feature = "diarization")]
impl MelSpectrogramGenerator {
    /// Create a new mel spectrogram generator with the given configuration
    pub fn new(config: MelConfig) -> Result<Self, DiarizationError> {
        // Create Hann window
        let window: Vec<f32> = (0..config.win_length)
            .map(|i| {
                0.5 * (1.0 - (2.0 * PI * i as f32 / (config.win_length - 1) as f32).cos())
            })
            .collect();

        // Create mel filterbank
        let mel_filterbank = create_mel_filterbank(
            config.n_mels,
            config.n_fft / 2 + 1,
            config.sample_rate as f32,
            config.fmin,
            config.fmax,
        );

        // Create FFT planner
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(config.n_fft);

        // Pre-allocate buffers
        let fft_input = vec![0.0f32; config.n_fft];
        let fft_output = vec![realfft::num_complex::Complex::new(0.0, 0.0); config.n_fft / 2 + 1];

        Ok(Self {
            config,
            fft,
            mel_filterbank,
            window,
            fft_input,
            fft_output,
        })
    }

    /// Compute mel spectrogram from audio samples
    ///
    /// # Arguments
    /// * `audio` - Audio samples at 16kHz, mono, f32
    ///
    /// # Returns
    /// Mel spectrogram as Vec<Vec<f32>> where outer dim is time frames
    /// and inner dim is mel bands (80)
    pub fn compute(&mut self, audio: &[f32]) -> Result<Vec<Vec<f32>>, DiarizationError> {
        if audio.is_empty() {
            return Err(DiarizationError::InvalidAudio("Empty audio".to_string()));
        }

        let n_frames = if audio.len() >= self.config.win_length {
            1 + (audio.len() - self.config.win_length) / self.config.hop_length
        } else {
            1
        };

        let mut mel_spec = Vec::with_capacity(n_frames);

        for frame_idx in 0..n_frames {
            let start = frame_idx * self.config.hop_length;
            let end = (start + self.config.win_length).min(audio.len());

            // Clear and fill FFT input with windowed audio
            self.fft_input.fill(0.0);
            for (i, &sample) in audio[start..end].iter().enumerate() {
                if i < self.window.len() {
                    self.fft_input[i] = sample * self.window[i];
                }
            }

            // Perform FFT
            self.fft
                .process(&mut self.fft_input, &mut self.fft_output)
                .map_err(|e| DiarizationError::MelError(format!("FFT failed: {}", e)))?;

            // Compute power spectrum
            let power_spec: Vec<f32> = self
                .fft_output
                .iter()
                .map(|c| c.re * c.re + c.im * c.im)
                .collect();

            // Apply mel filterbank and take log
            let mel_frame: Vec<f32> = self
                .mel_filterbank
                .iter()
                .map(|filter| {
                    let energy: f32 = filter
                        .iter()
                        .zip(power_spec.iter())
                        .map(|(f, p)| f * p)
                        .sum();
                    (energy + self.config.log_offset).ln()
                })
                .collect();

            mel_spec.push(mel_frame);
        }

        Ok(mel_spec)
    }

    /// Compute total energy of the mel spectrogram (for silence detection)
    pub fn compute_energy(mel_spec: &[Vec<f32>]) -> f32 {
        mel_spec
            .iter()
            .flat_map(|frame| frame.iter())
            .map(|v| v.exp())
            .sum::<f32>()
            / (mel_spec.len().max(1) as f32)
    }
}

/// Convert frequency to mel scale
fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

/// Convert mel scale to frequency
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0)
}

/// Create a mel filterbank matrix
///
/// # Arguments
/// * `n_mels` - Number of mel bands (typically 80)
/// * `n_fft_bins` - Number of FFT bins (n_fft/2 + 1)
/// * `sample_rate` - Audio sample rate in Hz
/// * `fmin` - Minimum frequency for mel bands
/// * `fmax` - Maximum frequency for mel bands
///
/// # Returns
/// Vec of mel filters, each filter is a Vec of weights for FFT bins
fn create_mel_filterbank(
    n_mels: usize,
    n_fft_bins: usize,
    sample_rate: f32,
    fmin: f32,
    fmax: f32,
) -> Vec<Vec<f32>> {
    let mel_min = hz_to_mel(fmin);
    let mel_max = hz_to_mel(fmax);

    // Create n_mels + 2 equally spaced points in mel scale
    let mel_points: Vec<f32> = (0..=n_mels + 1)
        .map(|i| mel_min + (mel_max - mel_min) * (i as f32) / ((n_mels + 1) as f32))
        .collect();

    // Convert mel points back to Hz
    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    // Convert Hz points to FFT bin indices
    let fft_bin_points: Vec<f32> = hz_points
        .iter()
        .map(|&hz| (n_fft_bins as f32 - 1.0) * hz / (sample_rate / 2.0))
        .collect();

    // Create triangular filters
    let mut filterbank = Vec::with_capacity(n_mels);

    for i in 0..n_mels {
        let mut filter = vec![0.0f32; n_fft_bins];

        let left = fft_bin_points[i];
        let center = fft_bin_points[i + 1];
        let right = fft_bin_points[i + 2];

        for (bin, weight) in filter.iter_mut().enumerate() {
            let bin_f = bin as f32;

            if bin_f >= left && bin_f < center {
                // Rising edge
                *weight = (bin_f - left) / (center - left);
            } else if bin_f >= center && bin_f <= right {
                // Falling edge
                *weight = (right - bin_f) / (right - center);
            }
        }

        filterbank.push(filter);
    }

    filterbank
}

// Stub implementation when feature is not enabled
#[cfg(not(feature = "diarization"))]
pub struct MelSpectrogramGenerator;

#[cfg(not(feature = "diarization"))]
impl MelSpectrogramGenerator {
    pub fn new(_config: MelConfig) -> Result<Self, DiarizationError> {
        Err(DiarizationError::FeatureNotEnabled)
    }

    pub fn compute(&mut self, _audio: &[f32]) -> Result<Vec<Vec<f32>>, DiarizationError> {
        Err(DiarizationError::FeatureNotEnabled)
    }

    pub fn compute_energy(_mel_spec: &[Vec<f32>]) -> f32 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hz_to_mel() {
        // 0 Hz should map to 0 mel
        assert!((hz_to_mel(0.0) - 0.0).abs() < 1e-6);

        // 1000 Hz is approximately 1000 mel (by design of the scale)
        let mel_1000 = hz_to_mel(1000.0);
        assert!((mel_1000 - 1000.0).abs() < 50.0); // Within 50 mel
    }

    #[test]
    fn test_mel_to_hz_roundtrip() {
        for hz in [100.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0] {
            let mel = hz_to_mel(hz);
            let hz_back = mel_to_hz(mel);
            assert!(
                (hz - hz_back).abs() < 1e-3,
                "Roundtrip failed for {} Hz",
                hz
            );
        }
    }

    #[test]
    fn test_create_mel_filterbank() {
        let filterbank = create_mel_filterbank(80, 257, 16000.0, 20.0, 7600.0);

        // Should have 80 filters
        assert_eq!(filterbank.len(), 80);

        // Each filter should have 257 bins
        for filter in &filterbank {
            assert_eq!(filter.len(), 257);
        }

        // Filters should be non-negative
        for filter in &filterbank {
            for &weight in filter {
                assert!(weight >= 0.0);
            }
        }

        // Each filter should have some non-zero weights
        for filter in &filterbank {
            let sum: f32 = filter.iter().sum();
            assert!(sum > 0.0, "Filter should have non-zero weights");
        }
    }

    #[cfg(feature = "diarization")]
    #[test]
    fn test_mel_spectrogram_dimensions() {
        let config = MelConfig::default();
        let mut gen = MelSpectrogramGenerator::new(config.clone()).unwrap();

        // 1 second of audio at 16kHz
        let audio: Vec<f32> = vec![0.0; 16000];
        let mel = gen.compute(&audio).unwrap();

        // Should have approximately 100 frames (16000 / 160 hop)
        let expected_frames = 1 + (16000 - config.win_length) / config.hop_length;
        assert_eq!(mel.len(), expected_frames);

        // Each frame should have 80 mel bands
        for frame in &mel {
            assert_eq!(frame.len(), 80);
        }
    }

    #[cfg(feature = "diarization")]
    #[test]
    fn test_mel_spectrogram_silence() {
        let config = MelConfig::default();
        let mut gen = MelSpectrogramGenerator::new(config).unwrap();

        // Silence
        let audio: Vec<f32> = vec![0.0; 16000];
        let mel = gen.compute(&audio).unwrap();

        // Energy should be very low for silence
        let energy = MelSpectrogramGenerator::compute_energy(&mel);
        assert!(energy < 1e-3, "Silence should have very low energy");
    }

    #[cfg(feature = "diarization")]
    #[test]
    fn test_mel_spectrogram_tone() {
        let config = MelConfig::default();
        let mut gen = MelSpectrogramGenerator::new(config).unwrap();

        // Generate a 440 Hz sine wave
        let audio: Vec<f32> = (0..16000)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / 16000.0).sin() * 0.5)
            .collect();

        let mel = gen.compute(&audio).unwrap();

        // Energy should be higher than silence
        let energy = MelSpectrogramGenerator::compute_energy(&mel);
        assert!(
            energy > 0.1,
            "Tone should have measurable energy, got {}",
            energy
        );
    }
}
