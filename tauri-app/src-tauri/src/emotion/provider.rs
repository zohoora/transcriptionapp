//! Emotion detection provider using wav2small ONNX model.
//!
//! Wav2small is an ultra-lightweight speech emotion recognition model with only
//! 72K parameters that outputs dimensional emotion values (arousal, dominance, valence).

#[cfg(feature = "emotion")]
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during emotion detection
#[derive(Debug, Error)]
pub enum EmotionError {
    #[error("Failed to load model: {0}")]
    ModelLoadError(String),

    #[error("Inference error: {0}")]
    InferenceError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Feature not enabled")]
    FeatureNotEnabled,
}

/// Dimensional emotion result from wav2small
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionResult {
    /// Arousal level (0.0 = calm, 1.0 = excited/activated)
    pub arousal: f32,
    /// Dominance level (0.0 = submissive, 1.0 = dominant)
    pub dominance: f32,
    /// Valence level (0.0 = negative, 1.0 = positive)
    pub valence: f32,
}

impl EmotionResult {
    /// Create a new emotion result
    pub fn new(arousal: f32, dominance: f32, valence: f32) -> Self {
        Self {
            arousal,
            dominance,
            valence,
        }
    }

    /// Get a human-readable emotion label based on ADV values
    pub fn label(&self) -> &'static str {
        // Simple mapping based on arousal and valence quadrants
        let high_arousal = self.arousal > 0.5;
        let positive_valence = self.valence > 0.5;

        match (high_arousal, positive_valence) {
            (true, true) => "excited/happy",
            (true, false) => "angry/frustrated",
            (false, true) => "calm/content",
            (false, false) => "sad/tired",
        }
    }

    /// Get confidence (based on distance from neutral center)
    pub fn confidence(&self) -> f32 {
        let dist_arousal = (self.arousal - 0.5).abs();
        let dist_valence = (self.valence - 0.5).abs();
        // Confidence is higher when further from neutral
        ((dist_arousal.powi(2) + dist_valence.powi(2)).sqrt() / 0.707).min(1.0)
    }
}

impl Default for EmotionResult {
    fn default() -> Self {
        Self {
            arousal: 0.5,
            dominance: 0.5,
            valence: 0.5,
        }
    }
}

/// Configuration for emotion detection
#[derive(Debug, Clone)]
pub struct EmotionConfig {
    /// Path to the wav2small ONNX model
    pub model_path: std::path::PathBuf,
    /// Number of threads for ONNX inference
    pub n_threads: i32,
    /// Minimum audio samples for reliable emotion detection (0.5s at 16kHz)
    pub min_audio_samples: usize,
}

impl Default for EmotionConfig {
    fn default() -> Self {
        Self {
            model_path: std::path::PathBuf::new(),
            n_threads: 1,
            min_audio_samples: 8000, // 0.5 seconds at 16kHz
        }
    }
}

/// Emotion detection provider using wav2small
#[cfg(feature = "emotion")]
pub struct EmotionProvider {
    session: Session,
    config: EmotionConfig,
}

#[cfg(feature = "emotion")]
impl EmotionProvider {
    /// Create a new emotion provider
    pub fn new(config: EmotionConfig) -> Result<Self, EmotionError> {
        if !config.model_path.exists() {
            return Err(EmotionError::ModelLoadError(format!(
                "Model not found at {:?}",
                config.model_path
            )));
        }

        let session = Session::builder()
            .map_err(|e: ort::Error| EmotionError::ModelLoadError(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e: ort::Error| EmotionError::ModelLoadError(e.to_string()))?
            .with_intra_threads(config.n_threads as usize)
            .map_err(|e: ort::Error| EmotionError::ModelLoadError(e.to_string()))?
            .commit_from_file(&config.model_path)
            .map_err(|e: ort::Error| EmotionError::ModelLoadError(e.to_string()))?;

        tracing::info!(
            "Emotion provider initialized with model: {:?}",
            config.model_path
        );

        Ok(Self { session, config })
    }

    /// Detect emotion from audio samples
    ///
    /// # Arguments
    /// * `audio` - Audio samples at 16kHz mono, normalized to [-1, 1]
    ///
    /// # Returns
    /// Emotion result with arousal, dominance, and valence values
    pub fn detect(&mut self, audio: &[f32]) -> Result<EmotionResult, EmotionError> {
        if audio.len() < self.config.min_audio_samples {
            tracing::debug!(
                "Audio too short for reliable emotion detection: {} samples (min: {})",
                audio.len(),
                self.config.min_audio_samples
            );
            return Ok(EmotionResult::default());
        }

        // wav2small expects input shape [batch, time]
        let input_shape = [1_usize, audio.len()];

        let input_tensor = Value::from_array((input_shape, audio.to_vec()))
            .map_err(|e: ort::Error| EmotionError::InferenceError(e.to_string()))?;

        let outputs = self
            .session
            .run(ort::inputs![input_tensor])
            .map_err(|e: ort::Error| EmotionError::InferenceError(e.to_string()))?;

        // Extract ADV values from output
        // wav2small outputs [arousal, dominance, valence] as a single tensor
        let output = outputs
            .iter()
            .next()
            .ok_or_else(|| EmotionError::InferenceError("No output from model".to_string()))?;

        let output_tensor = output
            .1
            .try_extract_tensor::<f32>()
            .map_err(|e: ort::Error| EmotionError::InferenceError(e.to_string()))?;

        let values: Vec<f32> = output_tensor.1.iter().copied().collect();

        if values.len() >= 3 {
            let result = EmotionResult::new(
                values[0].clamp(0.0, 1.0),
                values[1].clamp(0.0, 1.0),
                values[2].clamp(0.0, 1.0),
            );

            tracing::debug!(
                "Emotion detected: {} (A:{:.2} D:{:.2} V:{:.2}, conf:{:.0}%)",
                result.label(),
                result.arousal,
                result.dominance,
                result.valence,
                result.confidence() * 100.0
            );

            Ok(result)
        } else {
            tracing::warn!("Unexpected output shape from emotion model: {:?}", values.len());
            Ok(EmotionResult::default())
        }
    }

    /// Check if the provider is ready
    pub fn is_ready(&self) -> bool {
        true
    }
}

// Stub implementation when feature is not enabled
#[cfg(not(feature = "emotion"))]
pub struct EmotionProvider;

#[cfg(not(feature = "emotion"))]
impl EmotionProvider {
    pub fn new(_config: EmotionConfig) -> Result<Self, EmotionError> {
        Err(EmotionError::FeatureNotEnabled)
    }

    pub fn detect(&mut self, _audio: &[f32]) -> Result<EmotionResult, EmotionError> {
        Err(EmotionError::FeatureNotEnabled)
    }

    pub fn is_ready(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emotion_result_label() {
        // High arousal, positive valence -> excited/happy
        let result = EmotionResult::new(0.8, 0.5, 0.7);
        assert_eq!(result.label(), "excited/happy");

        // High arousal, negative valence -> angry/frustrated
        let result = EmotionResult::new(0.8, 0.5, 0.3);
        assert_eq!(result.label(), "angry/frustrated");

        // Low arousal, positive valence -> calm/content
        let result = EmotionResult::new(0.2, 0.5, 0.7);
        assert_eq!(result.label(), "calm/content");

        // Low arousal, negative valence -> sad/tired
        let result = EmotionResult::new(0.2, 0.5, 0.3);
        assert_eq!(result.label(), "sad/tired");
    }

    #[test]
    fn test_emotion_result_confidence() {
        // Neutral emotions have low confidence
        let neutral = EmotionResult::new(0.5, 0.5, 0.5);
        assert!(neutral.confidence() < 0.1);

        // Extreme emotions have high confidence
        let extreme = EmotionResult::new(1.0, 0.5, 0.0);
        assert!(extreme.confidence() > 0.5);
    }

    #[test]
    fn test_default_config() {
        let config = EmotionConfig::default();
        assert_eq!(config.n_threads, 1);
        assert_eq!(config.min_audio_samples, 8000);
    }

    #[cfg(not(feature = "emotion"))]
    #[test]
    fn test_stub_provider() {
        let config = EmotionConfig::default();
        let result = EmotionProvider::new(config);
        assert!(matches!(result, Err(EmotionError::FeatureNotEnabled)));
    }
}
