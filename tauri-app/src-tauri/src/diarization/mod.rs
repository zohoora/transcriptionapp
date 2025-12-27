//! Speaker diarization module using ONNX-based ECAPA-TDNN embeddings.
//!
//! This module provides real-time speaker identification by:
//! 1. Converting audio to mel spectrograms
//! 2. Extracting speaker embeddings via ONNX model
//! 3. Clustering embeddings to assign speaker IDs

pub mod clustering;
pub mod config;
pub mod embedding;
pub mod mel;
pub mod provider;

pub use config::{ClusterConfig, DiarizationConfig};
pub use provider::DiarizationProvider;

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during diarization
#[derive(Debug, Error)]
pub enum DiarizationError {
    #[error("Failed to load ONNX model: {0}")]
    ModelLoadError(String),

    #[error("ONNX inference failed: {0}")]
    InferenceError(String),

    #[error("Invalid audio input: {0}")]
    InvalidAudio(String),

    #[error("Mel spectrogram computation failed: {0}")]
    MelError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Model not found at path: {0}")]
    ModelNotFound(PathBuf),

    #[error("Feature not enabled: diarization requires the 'diarization' feature")]
    FeatureNotEnabled,
}

#[cfg(feature = "diarization")]
impl From<ort::Error> for DiarizationError {
    fn from(e: ort::Error) -> Self {
        DiarizationError::InferenceError(e.to_string())
    }
}

/// L2-normalize a vector in place
pub fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Compute cosine similarity between two L2-normalized vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same length");
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_normalize() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);

        // Check it's unit length
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        let mut v = vec![0.0, 0.0, 0.0];
        l2_normalize(&mut v);
        // Should remain zero (no division by zero)
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![0.6, 0.8];
        let b = vec![0.6, 0.8];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }
}
