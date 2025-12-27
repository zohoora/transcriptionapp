//! ONNX-based speaker embedding extraction using WeSpeaker.
//!
//! Extracts 256-dimensional speaker embeddings from mel spectrograms.

use super::DiarizationError;
use std::path::Path;

#[cfg(feature = "diarization")]
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};

/// Speaker embedding dimension for WeSpeaker ResNet34 (model outputs 256-dim embeddings)
pub const EMBEDDING_DIM: usize = 256;

/// ONNX-based speaker embedding extractor
#[cfg(feature = "diarization")]
pub struct EmbeddingExtractor {
    session: Session,
}

#[cfg(feature = "diarization")]
impl EmbeddingExtractor {
    /// Create a new embedding extractor from an ONNX model file
    ///
    /// # Arguments
    /// * `model_path` - Path to the ONNX model file
    /// * `n_threads` - Number of threads for inference
    pub fn new(model_path: &Path, n_threads: i32) -> Result<Self, DiarizationError> {
        if !model_path.exists() {
            return Err(DiarizationError::ModelNotFound(model_path.to_path_buf()));
        }

        // Initialize ONNX Runtime session
        let session = Session::builder()
            .map_err(|e| DiarizationError::ModelLoadError(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| DiarizationError::ModelLoadError(e.to_string()))?
            .with_intra_threads(n_threads as usize)
            .map_err(|e| DiarizationError::ModelLoadError(e.to_string()))?
            .commit_from_file(model_path)
            .map_err(|e| DiarizationError::ModelLoadError(e.to_string()))?;

        tracing::info!(
            "Loaded speaker embedding model from {:?}",
            model_path
        );

        Ok(Self { session })
    }

    /// Extract speaker embedding from mel spectrogram
    ///
    /// # Arguments
    /// * `mel_spec` - Mel spectrogram as [frames][mel_bands]
    ///
    /// # Returns
    /// 256-dimensional speaker embedding vector
    pub fn extract(&mut self, mel_spec: &[Vec<f32>]) -> Result<Vec<f32>, DiarizationError> {
        if mel_spec.is_empty() {
            return Err(DiarizationError::InvalidAudio(
                "Empty mel spectrogram".to_string(),
            ));
        }

        let n_frames = mel_spec.len();
        let n_mels = mel_spec[0].len();

        // Wespeaker ECAPA-TDNN expects input shape: [batch, n_frames, n_mels]
        // mel_spec is already in [frames][mels] format, so flatten directly
        let input_data: Vec<f32> = mel_spec.iter().flat_map(|frame| frame.iter().copied()).collect();

        // Create input tensor with shape [1, n_frames, n_mels]
        let input_shape = [1_usize, n_frames, n_mels];

        let input_tensor = Value::from_array((input_shape, input_data))
            .map_err(|e: ort::Error| DiarizationError::InferenceError(e.to_string()))?;

        // Run inference
        let outputs = self
            .session
            .run(ort::inputs![input_tensor])
            .map_err(|e| DiarizationError::InferenceError(e.to_string()))?;

        // Extract embedding from output
        // WeSpeaker output shape is [batch, embedding_dim] = [1, 256]
        let output = outputs
            .iter()
            .next()
            .ok_or_else(|| DiarizationError::InferenceError("No output tensor".to_string()))?;

        let embedding_data = output
            .1
            .try_extract_tensor::<f32>()
            .map_err(|e| DiarizationError::InferenceError(e.to_string()))?;

        let embedding: Vec<f32> = embedding_data.1.iter().copied().collect();

        if embedding.len() != EMBEDDING_DIM {
            tracing::warn!(
                "Unexpected embedding dimension: {} (expected {})",
                embedding.len(),
                EMBEDDING_DIM
            );
        }

        Ok(embedding)
    }

    /// Get the expected embedding dimension
    pub fn embedding_dim(&self) -> usize {
        EMBEDDING_DIM
    }
}

// Stub implementation when feature is not enabled
#[cfg(not(feature = "diarization"))]
pub struct EmbeddingExtractor;

#[cfg(not(feature = "diarization"))]
impl EmbeddingExtractor {
    pub fn new(_model_path: &Path, _n_threads: i32) -> Result<Self, DiarizationError> {
        Err(DiarizationError::FeatureNotEnabled)
    }

    pub fn extract(&mut self, _mel_spec: &[Vec<f32>]) -> Result<Vec<f32>, DiarizationError> {
        Err(DiarizationError::FeatureNotEnabled)
    }

    pub fn embedding_dim(&self) -> usize {
        EMBEDDING_DIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_dim_constant() {
        assert_eq!(EMBEDDING_DIM, 256);
    }

    #[cfg(feature = "diarization")]
    #[test]
    fn test_extractor_model_not_found() {
        let result = EmbeddingExtractor::new(Path::new("/nonexistent/model.onnx"), 1);
        assert!(matches!(result, Err(DiarizationError::ModelNotFound(_))));
    }
}
