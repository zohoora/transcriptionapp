//! Speech enhancement provider using GTCRN ONNX model.
//!
//! GTCRN (Grouped Temporal Convolutional Recurrent Network) is an ultra-lightweight
//! speech enhancement model with only 48K parameters that runs in real-time.
//!
//! The model operates in the STFT domain and requires preprocessing:
//! 1. Compute STFT of input audio (512-point FFT, 256 hop, 512 window)
//! 2. Run GTCRN on magnitude/phase
//! 3. Compute inverse STFT to get enhanced audio

#[cfg(feature = "enhancement")]
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};
#[cfg(feature = "enhancement")]
use rustfft::{num_complex::Complex, FftPlanner};
use thiserror::Error;

/// Errors that can occur during speech enhancement
#[derive(Debug, Error)]
pub enum EnhancementError {
    #[error("Failed to load model: {0}")]
    ModelLoadError(String),

    #[error("Inference error: {0}")]
    InferenceError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Feature not enabled")]
    FeatureNotEnabled,
}

/// Configuration for speech enhancement
#[derive(Debug, Clone)]
pub struct EnhancementConfig {
    /// Path to the GTCRN ONNX model
    pub model_path: std::path::PathBuf,
    /// Number of threads for ONNX inference
    pub n_threads: i32,
}

impl Default for EnhancementConfig {
    fn default() -> Self {
        Self {
            model_path: std::path::PathBuf::new(),
            n_threads: 1,
        }
    }
}

/// STFT parameters for GTCRN
#[cfg(feature = "enhancement")]
const FFT_SIZE: usize = 512;
#[cfg(feature = "enhancement")]
const HOP_SIZE: usize = 256;
#[cfg(feature = "enhancement")]
const FREQ_BINS: usize = FFT_SIZE / 2 + 1; // 257

/// Cache tensor sizes for GTCRN streaming
#[cfg(feature = "enhancement")]
const CONV_CACHE_SIZE: usize = 2 * 16 * 16 * 33;  // [2, 1, 16, 16, 33] = 16896
#[cfg(feature = "enhancement")]
const TRA_CACHE_SIZE: usize = (2 * 3) * 16;     // [2, 3, 1, 1, 16] = 96
#[cfg(feature = "enhancement")]
const INTER_CACHE_SIZE: usize = 2 * 33 * 16;      // [2, 1, 33, 16] = 1056

/// Speech enhancement provider using GTCRN
#[cfg(feature = "enhancement")]
pub struct EnhancementProvider {
    session: Session,
    #[allow(dead_code)]
    config: EnhancementConfig,
    fft_planner: FftPlanner<f32>,
    window: Vec<f32>,
    // Streaming cache state (persists between frames)
    conv_cache: Vec<f32>,
    tra_cache: Vec<f32>,
    inter_cache: Vec<f32>,
}

#[cfg(feature = "enhancement")]
impl EnhancementProvider {
    /// Create a new enhancement provider
    pub fn new(config: EnhancementConfig) -> Result<Self, EnhancementError> {
        if !config.model_path.exists() {
            return Err(EnhancementError::ModelLoadError(format!(
                "Model not found at {:?}",
                config.model_path
            )));
        }

        let session = Session::builder()
            .map_err(|e: ort::Error| EnhancementError::ModelLoadError(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e: ort::Error| EnhancementError::ModelLoadError(e.to_string()))?
            .with_intra_threads(config.n_threads as usize)
            .map_err(|e: ort::Error| EnhancementError::ModelLoadError(e.to_string()))?
            .commit_from_file(&config.model_path)
            .map_err(|e: ort::Error| EnhancementError::ModelLoadError(e.to_string()))?;

        // Create sqrt-Hann window (required by GTCRN for proper COLA reconstruction)
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                (0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos())).sqrt()
            })
            .collect();

        // Initialize cache tensors to zeros
        let conv_cache = vec![0.0f32; CONV_CACHE_SIZE];
        let tra_cache = vec![0.0f32; TRA_CACHE_SIZE];
        let inter_cache = vec![0.0f32; INTER_CACHE_SIZE];

        tracing::info!(
            "Enhancement provider initialized with model: {:?}",
            config.model_path
        );

        Ok(Self {
            session,
            config,
            fft_planner: FftPlanner::new(),
            window,
            conv_cache,
            tra_cache,
            inter_cache,
        })
    }

    /// Reset cache state (call before processing a new utterance)
    pub fn reset_cache(&mut self) {
        self.conv_cache.fill(0.0);
        self.tra_cache.fill(0.0);
        self.inter_cache.fill(0.0);
    }

    /// Compute STFT of one frame of audio
    /// Returns (real, imag) components for freq bins 0..257
    fn stft_frame(&mut self, audio: &[f32], frame_start: usize) -> (Vec<f32>, Vec<f32>) {
        let fft = self.fft_planner.plan_fft_forward(FFT_SIZE);

        // Apply window and create complex buffer
        let mut buffer: Vec<Complex<f32>> = (0..FFT_SIZE)
            .map(|i| {
                let sample = if frame_start + i < audio.len() {
                    audio[frame_start + i] * self.window[i]
                } else {
                    0.0
                };
                Complex::new(sample, 0.0)
            })
            .collect();

        // Compute FFT
        fft.process(&mut buffer);

        // Extract real and imag for positive frequencies
        let mut real = Vec::with_capacity(FREQ_BINS);
        let mut imag = Vec::with_capacity(FREQ_BINS);

        for bin in buffer.iter().take(FREQ_BINS) {
            real.push(bin.re);
            imag.push(bin.im);
        }

        (real, imag)
    }

    /// Compute STFT of audio - returns (real, imag) for all frames
    #[allow(dead_code)]
    fn stft(&mut self, audio: &[f32]) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let fft = self.fft_planner.plan_fft_forward(FFT_SIZE);
        let num_frames = (audio.len().saturating_sub(FFT_SIZE)) / HOP_SIZE + 1;

        let mut all_real = Vec::with_capacity(num_frames);
        let mut all_imag = Vec::with_capacity(num_frames);

        for frame_idx in 0..num_frames {
            let start = frame_idx * HOP_SIZE;
            let end = (start + FFT_SIZE).min(audio.len());

            // Apply window and create complex buffer
            let mut buffer: Vec<Complex<f32>> = (0..FFT_SIZE)
                .map(|i| {
                    let sample = if start + i < end {
                        audio[start + i] * self.window[i]
                    } else {
                        0.0
                    };
                    Complex::new(sample, 0.0)
                })
                .collect();

            // Compute FFT
            fft.process(&mut buffer);

            // Extract real and imag for positive frequencies
            let mut frame_real = Vec::with_capacity(FREQ_BINS);
            let mut frame_imag = Vec::with_capacity(FREQ_BINS);

            for bin in buffer.iter().take(FREQ_BINS) {
                frame_real.push(bin.re);
                frame_imag.push(bin.im);
            }

            all_real.push(frame_real);
            all_imag.push(frame_imag);
        }

        (all_real, all_imag)
    }

    /// Compute inverse STFT from real/imag components
    fn istft(&mut self, all_real: &[Vec<f32>], all_imag: &[Vec<f32>], original_len: usize) -> Vec<f32> {
        let ifft = self.fft_planner.plan_fft_inverse(FFT_SIZE);
        let num_frames = all_real.len();
        let output_len = (num_frames - 1) * HOP_SIZE + FFT_SIZE;

        let mut output = vec![0.0f32; output_len];
        let mut window_sum = vec![0.0f32; output_len];

        for (frame_idx, (real, imag)) in all_real.iter().zip(all_imag.iter()).enumerate() {
            // Reconstruct complex spectrum from real/imag
            let mut buffer: Vec<Complex<f32>> = (0..FFT_SIZE)
                .map(|i| {
                    if i < FREQ_BINS {
                        Complex::new(real[i], imag[i])
                    } else {
                        // Mirror for negative frequencies (conjugate symmetry)
                        let mirror_idx = FFT_SIZE - i;
                        Complex::new(real[mirror_idx], -imag[mirror_idx])
                    }
                })
                .collect();

            // Compute IFFT
            ifft.process(&mut buffer);

            // Overlap-add with window
            let start = frame_idx * HOP_SIZE;
            for (i, sample) in buffer.iter().enumerate() {
                if start + i < output_len {
                    output[start + i] += sample.re * self.window[i] / FFT_SIZE as f32;
                    window_sum[start + i] += self.window[i] * self.window[i];
                }
            }
        }

        // Normalize by window sum
        for (out, win) in output.iter_mut().zip(window_sum.iter()) {
            if *win > 1e-8 {
                *out /= win;
            }
        }

        // Trim to original length
        output.truncate(original_len);
        output
    }

    /// Enhance a single STFT frame using the GTCRN model
    ///
    /// This method processes one frame at a time, maintaining internal cache state
    /// for proper streaming behavior.
    ///
    /// # Arguments
    /// * `real` - Real components of STFT frame (257 values)
    /// * `imag` - Imaginary components of STFT frame (257 values)
    ///
    /// # Returns
    /// Enhanced (real, imag) components
    fn enhance_frame(&mut self, real: &[f32], imag: &[f32]) -> Result<(Vec<f32>, Vec<f32>), EnhancementError> {
        assert_eq!(real.len(), FREQ_BINS);
        assert_eq!(imag.len(), FREQ_BINS);

        // Build input tensor: [1, 257, 1, 2] = [batch, freq_bins, frames, real+imag]
        let mut mix_data = Vec::with_capacity(FREQ_BINS * 2);
        for i in 0..FREQ_BINS {
            mix_data.push(real[i]);
            mix_data.push(imag[i]);
        }

        // Create tensors
        let mix_tensor = Value::from_array(([1_usize, FREQ_BINS, 1, 2], mix_data))
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;

        let conv_cache_tensor = Value::from_array(([2_usize, 1, 16, 16, 33], self.conv_cache.clone()))
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;

        let tra_cache_tensor = Value::from_array(([2_usize, 3, 1, 1, 16], self.tra_cache.clone()))
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;

        let inter_cache_tensor = Value::from_array(([2_usize, 1, 33, 16], self.inter_cache.clone()))
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;

        // Run inference
        let outputs = self.session
            .run(ort::inputs![
                "mix" => mix_tensor,
                "conv_cache" => conv_cache_tensor,
                "tra_cache" => tra_cache_tensor,
                "inter_cache" => inter_cache_tensor
            ])
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;

        // Extract outputs: enh (enhanced frame), and updated caches
        // Output names: "enh", "new_conv_cache", "new_tra_cache", "new_inter_cache"
        let enh_output = outputs.get("enh")
            .ok_or_else(|| EnhancementError::InferenceError("Missing 'enh' output".to_string()))?;
        let enh_tensor = enh_output.try_extract_tensor::<f32>()
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;
        let enh_data: Vec<f32> = enh_tensor.1.to_vec();

        // Update caches from outputs
        if let Some(conv_out) = outputs.get("new_conv_cache") {
            if let Ok(tensor) = conv_out.try_extract_tensor::<f32>() {
                self.conv_cache = tensor.1.to_vec();
            }
        }
        if let Some(tra_out) = outputs.get("new_tra_cache") {
            if let Ok(tensor) = tra_out.try_extract_tensor::<f32>() {
                self.tra_cache = tensor.1.to_vec();
            }
        }
        if let Some(inter_out) = outputs.get("new_inter_cache") {
            if let Ok(tensor) = inter_out.try_extract_tensor::<f32>() {
                self.inter_cache = tensor.1.to_vec();
            }
        }

        // Parse enhanced output: [1, 257, 1, 2] -> split into real/imag
        let mut enh_real = Vec::with_capacity(FREQ_BINS);
        let mut enh_imag = Vec::with_capacity(FREQ_BINS);

        for i in 0..FREQ_BINS {
            enh_real.push(enh_data[i * 2]);
            enh_imag.push(enh_data[i * 2 + 1]);
        }

        Ok((enh_real, enh_imag))
    }

    /// Enhance/denoise audio samples
    ///
    /// # Arguments
    /// * `audio` - Audio samples at 16kHz mono, normalized to [-1, 1]
    ///
    /// # Returns
    /// Enhanced audio samples at the same sample rate
    pub fn enhance(&mut self, audio: &[f32]) -> Result<Vec<f32>, EnhancementError> {
        if audio.len() < FFT_SIZE {
            // Audio too short for enhancement, return as-is
            return Ok(audio.to_vec());
        }

        // Reset caches for new utterance
        self.reset_cache();

        // Compute number of frames
        let num_frames = (audio.len().saturating_sub(FFT_SIZE)) / HOP_SIZE + 1;

        if num_frames == 0 {
            return Ok(audio.to_vec());
        }

        // Process frame-by-frame with streaming enhancement
        let mut enhanced_real = Vec::with_capacity(num_frames);
        let mut enhanced_imag = Vec::with_capacity(num_frames);

        for frame_idx in 0..num_frames {
            let frame_start = frame_idx * HOP_SIZE;

            // Compute STFT for this frame
            let (real, imag) = self.stft_frame(audio, frame_start);

            // Enhance the frame
            let (enh_real, enh_imag) = self.enhance_frame(&real, &imag)?;

            enhanced_real.push(enh_real);
            enhanced_imag.push(enh_imag);
        }

        // Reconstruct audio via inverse STFT with overlap-add
        let enhanced_audio = self.istft(&enhanced_real, &enhanced_imag, audio.len());

        tracing::info!(
            "GTCRN enhanced utterance: {} samples, {} frames",
            audio.len(),
            num_frames
        );

        Ok(enhanced_audio)
    }

    /// Check if the provider is ready
    pub fn is_ready(&self) -> bool {
        true
    }
}

// Stub implementation when feature is not enabled
#[cfg(not(feature = "enhancement"))]
pub struct EnhancementProvider;

#[cfg(not(feature = "enhancement"))]
impl EnhancementProvider {
    pub fn new(_config: EnhancementConfig) -> Result<Self, EnhancementError> {
        Err(EnhancementError::FeatureNotEnabled)
    }

    pub fn enhance(&mut self, _audio: &[f32]) -> Result<Vec<f32>, EnhancementError> {
        Err(EnhancementError::FeatureNotEnabled)
    }

    pub fn is_ready(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EnhancementConfig::default();
        assert_eq!(config.n_threads, 1);
    }

    #[cfg(not(feature = "enhancement"))]
    #[test]
    fn test_stub_provider() {
        let config = EnhancementConfig::default();
        let result = EnhancementProvider::new(config);
        assert!(matches!(result, Err(EnhancementError::FeatureNotEnabled)));
    }

    // STFT/iSTFT unit tests (don't require ONNX model)
    #[cfg(feature = "enhancement")]
    mod stft_tests {
        use super::*;
        use std::f32::consts::PI;

        /// Generate a simple sine wave for testing
        fn generate_sine(samples: usize, freq: f32, sample_rate: f32) -> Vec<f32> {
            (0..samples)
                .map(|i| (2.0 * PI * freq * i as f32 / sample_rate).sin())
                .collect()
        }

        /// Generate speech-like signal (mixed frequencies)
        fn generate_speech_signal(samples: usize) -> Vec<f32> {
            let sample_rate = 16000.0;
            (0..samples)
                .map(|i| {
                    let t = i as f32 / sample_rate;
                    let f1 = (2.0 * PI * 200.0 * t).sin() * 0.4;
                    let f2 = (2.0 * PI * 400.0 * t).sin() * 0.3;
                    let f3 = (2.0 * PI * 800.0 * t).sin() * 0.2;
                    f1 + f2 + f3
                })
                .collect()
        }

        #[test]
        fn test_stft_frame_count() {
            // For FFT_SIZE=512, HOP_SIZE=256:
            // num_frames = (audio_len - FFT_SIZE) / HOP_SIZE + 1

            // Test case: 1024 samples -> (1024-512)/256 + 1 = 3 frames
            let audio = vec![0.0f32; 1024];
            let expected_frames = 3;

            // Create a minimal provider just to test STFT
            // We need to manually compute STFT without full provider
            let num_frames = (audio.len().saturating_sub(FFT_SIZE)) / HOP_SIZE + 1;
            assert_eq!(num_frames, expected_frames);

            // Test case: 512 samples -> 1 frame (minimum)
            let audio_min = vec![0.0f32; 512];
            let num_frames_min = (audio_min.len().saturating_sub(FFT_SIZE)) / HOP_SIZE + 1;
            assert_eq!(num_frames_min, 1);

            // Test case: 2048 samples -> (2048-512)/256 + 1 = 7 frames
            let audio_2k = vec![0.0f32; 2048];
            let num_frames_2k = (audio_2k.len().saturating_sub(FFT_SIZE)) / HOP_SIZE + 1;
            assert_eq!(num_frames_2k, 7);
        }

        #[test]
        fn test_sqrt_hann_window() {
            // Create sqrt-Hann window
            let window: Vec<f32> = (0..FFT_SIZE)
                .map(|i| {
                    (0.5 * (1.0 - (2.0 * PI * i as f32 / FFT_SIZE as f32).cos())).sqrt()
                })
                .collect();

            // Properties of sqrt-Hann:
            // 1. Length should be FFT_SIZE
            assert_eq!(window.len(), FFT_SIZE);

            // 2. First and last samples should be ~0
            assert!(window[0].abs() < 0.01);
            assert!(window[FFT_SIZE - 1].abs() < 0.01);

            // 3. Middle sample should be ~1 (peak of sqrt-Hann)
            assert!((window[FFT_SIZE / 2] - 1.0).abs() < 0.01);

            // 4. Window should be approximately symmetric around center
            // Note: periodic Hann is not perfectly symmetric at endpoints
            for i in 1..FFT_SIZE / 2 {
                let mirror = FFT_SIZE - i;
                assert!(
                    (window[i] - window[mirror]).abs() < 1e-5,
                    "Symmetry failed at i={}: {} vs {}",
                    i, window[i], window[mirror]
                );
            }

            // 5. COLA property: overlapped windows should sum to constant
            // For 50% overlap (HOP_SIZE = FFT_SIZE/2), sqrt-Hann^2 should sum to 1
            let overlap_sum: f32 = (0..HOP_SIZE)
                .map(|i| window[i].powi(2) + window[i + HOP_SIZE].powi(2))
                .sum::<f32>() / HOP_SIZE as f32;
            assert!((overlap_sum - 1.0).abs() < 0.01, "COLA property failed: sum = {}", overlap_sum);
        }

        #[test]
        fn test_stft_istft_reconstruction() {
            use rustfft::{num_complex::Complex, FftPlanner};

            // Generate test signal
            let audio = generate_sine(2048, 440.0, 16000.0);
            let original_len = audio.len();

            // Create sqrt-Hann window
            let window: Vec<f32> = (0..FFT_SIZE)
                .map(|i| {
                    (0.5 * (1.0 - (2.0 * PI * i as f32 / FFT_SIZE as f32).cos())).sqrt()
                })
                .collect();

            let mut planner = FftPlanner::new();
            let fft = planner.plan_fft_forward(FFT_SIZE);
            let ifft = planner.plan_fft_inverse(FFT_SIZE);

            // STFT
            let num_frames = (audio.len().saturating_sub(FFT_SIZE)) / HOP_SIZE + 1;
            let mut frames: Vec<Vec<Complex<f32>>> = Vec::with_capacity(num_frames);

            for frame_idx in 0..num_frames {
                let start = frame_idx * HOP_SIZE;
                let mut buffer: Vec<Complex<f32>> = (0..FFT_SIZE)
                    .map(|i| {
                        let sample = if start + i < audio.len() {
                            audio[start + i] * window[i]
                        } else {
                            0.0
                        };
                        Complex::new(sample, 0.0)
                    })
                    .collect();

                fft.process(&mut buffer);
                frames.push(buffer);
            }

            // iSTFT with overlap-add
            let output_len = (num_frames - 1) * HOP_SIZE + FFT_SIZE;
            let mut output = vec![0.0f32; output_len];
            let mut window_sum = vec![0.0f32; output_len];

            for (frame_idx, spectrum) in frames.iter().enumerate() {
                let mut buffer = spectrum.clone();
                ifft.process(&mut buffer);

                let start = frame_idx * HOP_SIZE;
                for (i, sample) in buffer.iter().enumerate() {
                    if start + i < output_len {
                        output[start + i] += sample.re * window[i] / FFT_SIZE as f32;
                        window_sum[start + i] += window[i] * window[i];
                    }
                }
            }

            // Normalize by window sum
            for (out, win) in output.iter_mut().zip(window_sum.iter()) {
                if *win > 1e-8 {
                    *out /= win;
                }
            }

            output.truncate(original_len);

            // Verify reconstruction
            assert_eq!(output.len(), audio.len());

            // Skip edge samples (affected by windowing) and check middle
            let start_check = FFT_SIZE;
            let end_check = audio.len() - FFT_SIZE;

            let mut max_error: f32 = 0.0;
            let mut total_error: f32 = 0.0;

            for i in start_check..end_check {
                let error = (audio[i] - output[i]).abs();
                max_error = max_error.max(error);
                total_error += error;
            }

            let avg_error = total_error / (end_check - start_check) as f32;

            // Reconstruction should be very accurate (< 1% error)
            assert!(max_error < 0.01, "Max reconstruction error too high: {}", max_error);
            assert!(avg_error < 0.001, "Avg reconstruction error too high: {}", avg_error);
        }

        #[test]
        fn test_stft_short_audio() {
            // Audio shorter than FFT_SIZE should produce 0 or 1 frames
            let audio = vec![0.0f32; 256];
            let num_frames = (audio.len().saturating_sub(FFT_SIZE)) / HOP_SIZE + 1;
            // saturating_sub(256 - 512) = 0, so 0/256 + 1 = 1
            // But 256 < 512, so we can't do a full frame
            // This depends on implementation - just ensure no panic
            assert!(num_frames <= 1);
        }
    }

    // Integration tests requiring ONNX model
    // Note: These tests require ORT_DYLIB_PATH to be set to the ONNX Runtime library
    #[cfg(feature = "enhancement")]
    mod integration_tests {
        use super::*;
        use std::f32::consts::PI;
        use std::path::PathBuf;

        /// Check if ONNX Runtime is available
        fn ort_available() -> bool {
            // ORT with load-dynamic requires ORT_DYLIB_PATH to be set
            std::env::var("ORT_DYLIB_PATH").map(|p| std::path::Path::new(&p).exists()).unwrap_or(false)
        }

        /// Get the path to the enhancement model
        fn get_model_path() -> Option<PathBuf> {
            // First check if ONNX Runtime is available
            if !ort_available() {
                return None;
            }

            let model_path = dirs::home_dir()?.join(".transcriptionapp/models/gtcrn_simple.onnx");
            if model_path.exists() {
                Some(model_path)
            } else {
                None
            }
        }

        /// Generate noisy speech-like signal for testing
        fn generate_noisy_speech(samples: usize) -> Vec<f32> {
            let sample_rate = 16000.0;
            (0..samples)
                .map(|i| {
                    let t = i as f32 / sample_rate;
                    // Speech fundamentals
                    let speech = (2.0 * PI * 200.0 * t).sin() * 0.4
                        + (2.0 * PI * 400.0 * t).sin() * 0.3
                        + (2.0 * PI * 800.0 * t).sin() * 0.2;
                    // Add some noise
                    let noise = (t * 12345.6789).sin() * 0.1;
                    (speech + noise).clamp(-1.0, 1.0)
                })
                .collect()
        }

        #[test]
        fn test_enhance_preserves_length() {
            let model_path = match get_model_path() {
                Some(p) => p,
                None => {
                    eprintln!("Skipping test_enhance_preserves_length: model not found");
                    return;
                }
            };

            let config = EnhancementConfig {
                model_path,
                n_threads: 1,
            };

            let mut provider = match EnhancementProvider::new(config) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Skipping test: failed to create provider: {}", e);
                    return;
                }
            };

            // Test various audio lengths
            for samples in [1024, 2048, 4096, 16000, 32000] {
                let audio = generate_noisy_speech(samples);
                let enhanced = provider.enhance(&audio).expect("Enhancement failed");
                assert_eq!(
                    enhanced.len(),
                    audio.len(),
                    "Output length {} != input length {} for {} samples",
                    enhanced.len(),
                    audio.len(),
                    samples
                );
            }
        }

        #[test]
        fn test_enhance_edge_cases() {
            let model_path = match get_model_path() {
                Some(p) => p,
                None => {
                    eprintln!("Skipping test_enhance_edge_cases: model not found");
                    return;
                }
            };

            let config = EnhancementConfig {
                model_path,
                n_threads: 1,
            };

            let mut provider = match EnhancementProvider::new(config) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Skipping test: failed to create provider: {}", e);
                    return;
                }
            };

            // Test edge cases
            // Too short - should return as-is
            let short_audio = vec![0.5f32; 256];
            let result = provider.enhance(&short_audio);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), short_audio);

            // Exactly FFT_SIZE
            let exact_audio = generate_noisy_speech(FFT_SIZE);
            let result = provider.enhance(&exact_audio);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().len(), exact_audio.len());

            // FFT_SIZE + 1
            let audio_plus1 = generate_noisy_speech(FFT_SIZE + 1);
            let result = provider.enhance(&audio_plus1);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().len(), audio_plus1.len());

            // Odd length
            let odd_audio = generate_noisy_speech(1023);
            let result = provider.enhance(&odd_audio);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().len(), odd_audio.len());
        }

        #[test]
        fn test_enhance_output_range() {
            let model_path = match get_model_path() {
                Some(p) => p,
                None => {
                    eprintln!("Skipping test_enhance_output_range: model not found");
                    return;
                }
            };

            let config = EnhancementConfig {
                model_path,
                n_threads: 1,
            };

            let mut provider = match EnhancementProvider::new(config) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Skipping test: failed to create provider: {}", e);
                    return;
                }
            };

            let audio = generate_noisy_speech(16000); // 1 second
            let enhanced = provider.enhance(&audio).expect("Enhancement failed");

            // Output should be in reasonable range (not exploding)
            let max_val = enhanced.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
            assert!(
                max_val < 10.0,
                "Enhanced output too large: max = {}",
                max_val
            );

            // Should have some energy (not all zeros)
            let energy: f32 = enhanced.iter().map(|x| x * x).sum();
            assert!(energy > 0.01, "Enhanced output has no energy");
        }

        #[test]
        fn test_enhance_performance() {
            let model_path = match get_model_path() {
                Some(p) => p,
                None => {
                    eprintln!("Skipping test_enhance_performance: model not found");
                    return;
                }
            };

            let config = EnhancementConfig {
                model_path,
                n_threads: 1,
            };

            let mut provider = match EnhancementProvider::new(config) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Skipping test: failed to create provider: {}", e);
                    return;
                }
            };

            // 1 second of audio at 16kHz
            let audio = generate_noisy_speech(16000);

            let start = std::time::Instant::now();
            let _enhanced = provider.enhance(&audio).expect("Enhancement failed");
            let elapsed = start.elapsed();

            // Should process 1 second of audio in less than 1 second (real-time capable)
            // We'll use a generous threshold of 500ms for CI environments
            println!(
                "Enhanced 1 second of audio in {:?} ({:.2}x real-time)",
                elapsed,
                1000.0 / elapsed.as_millis() as f32
            );
            assert!(
                elapsed.as_millis() < 1000,
                "Enhancement took too long: {:?}",
                elapsed
            );
        }

        #[test]
        fn test_cache_reset() {
            let model_path = match get_model_path() {
                Some(p) => p,
                None => {
                    eprintln!("Skipping test_cache_reset: model not found");
                    return;
                }
            };

            let config = EnhancementConfig {
                model_path,
                n_threads: 1,
            };

            let mut provider = match EnhancementProvider::new(config) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Skipping test: failed to create provider: {}", e);
                    return;
                }
            };

            let audio = generate_noisy_speech(16000);

            // Process once
            let result1 = provider.enhance(&audio).expect("First enhancement failed");

            // Process again - should give same result (caches reset each time)
            let result2 = provider.enhance(&audio).expect("Second enhancement failed");

            // Results should be identical since caches are reset
            assert_eq!(result1.len(), result2.len());
            for (a, b) in result1.iter().zip(result2.iter()) {
                assert!(
                    (a - b).abs() < 1e-5,
                    "Results differ after cache reset"
                );
            }
        }
    }
}
