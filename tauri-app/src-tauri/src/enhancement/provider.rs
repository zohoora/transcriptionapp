//! Speech enhancement provider using GTCRN ONNX model.
//!
//! GTCRN (Grouped Temporal Convolutional Recurrent Network) is an ultra-lightweight
//! speech enhancement model with only 48K parameters that runs in real-time.

#[cfg(feature = "enhancement")]
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};
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

/// Speech enhancement provider using GTCRN
#[cfg(feature = "enhancement")]
pub struct EnhancementProvider {
    session: Session,
    #[allow(dead_code)]
    config: EnhancementConfig,
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

        tracing::info!(
            "Enhancement provider initialized with model: {:?}",
            config.model_path
        );

        Ok(Self { session, config })
    }

    /// Enhance/denoise audio samples
    ///
    /// # Arguments
    /// * `audio` - Audio samples at 16kHz mono, normalized to [-1, 1]
    ///
    /// # Returns
    /// Enhanced audio samples at the same sample rate
    pub fn enhance(&mut self, audio: &[f32]) -> Result<Vec<f32>, EnhancementError> {
        if audio.is_empty() {
            return Ok(Vec::new());
        }

        // GTCRN expects input shape [batch, time]
        let input_shape = [1_usize, audio.len()];

        let input_tensor = Value::from_array((input_shape, audio.to_vec()))
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;

        let outputs = self
            .session
            .run(ort::inputs![input_tensor])
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;

        // Extract the enhanced audio from output
        let output = outputs
            .iter()
            .next()
            .ok_or_else(|| EnhancementError::InferenceError("No output from model".to_string()))?;

        let output_tensor = output
            .1
            .try_extract_tensor::<f32>()
            .map_err(|e: ort::Error| EnhancementError::InferenceError(e.to_string()))?;

        let enhanced: Vec<f32> = output_tensor.1.iter().copied().collect();

        tracing::debug!(
            "Enhanced {} samples -> {} samples",
            audio.len(),
            enhanced.len()
        );

        Ok(enhanced)
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
}
