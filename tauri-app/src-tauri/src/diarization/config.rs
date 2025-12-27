//! Configuration structures for speaker diarization.

use std::path::PathBuf;

/// Configuration for the diarization provider
#[derive(Debug, Clone)]
pub struct DiarizationConfig {
    /// Path to the ONNX speaker embedding model
    pub model_path: PathBuf,

    /// Cosine similarity threshold for same-speaker decision (0.0-1.0)
    /// Higher values require more similarity to match existing speaker
    pub similarity_threshold: f32,

    /// Minimum similarity to consider a match when at max speakers
    pub min_similarity: f32,

    /// Maximum number of speakers to track
    pub max_speakers: usize,

    /// Number of threads for ONNX inference
    pub n_threads: i32,

    /// Minimum audio duration in samples for reliable embedding (16kHz)
    /// Default: 8000 samples = 500ms
    pub min_audio_samples: usize,

    /// Minimum energy threshold to process audio (log scale)
    /// Audio below this is considered silence
    pub min_energy_threshold: f32,
}

impl Default for DiarizationConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            similarity_threshold: 0.3,
            min_similarity: 0.5,
            max_speakers: 10,
            n_threads: 2,
            min_audio_samples: 8000, // 500ms at 16kHz
            min_energy_threshold: -10.0,
        }
    }
}

impl DiarizationConfig {
    /// Create a new config with the specified model path
    pub fn with_model_path(model_path: PathBuf) -> Self {
        Self {
            model_path,
            ..Default::default()
        }
    }
}

/// Configuration for the speaker clustering algorithm
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// Cosine similarity threshold for same-speaker decision
    pub similarity_threshold: f32,

    /// Minimum similarity to consider a match when at max speakers
    pub min_similarity: f32,

    /// Maximum number of speakers to track
    pub max_speakers: usize,

    /// EMA alpha for centroid updates (after stabilization)
    /// Lower values = more stable centroids, higher = more adaptive
    pub centroid_ema_alpha: f32,

    /// Minimum embeddings before centroid is considered "stable"
    /// Before this, simple averaging is used instead of EMA
    pub min_embeddings_stable: u32,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.3,
            min_similarity: 0.5,
            max_speakers: 10,
            centroid_ema_alpha: 0.3,
            min_embeddings_stable: 3,
        }
    }
}

impl ClusterConfig {
    /// Create a cluster config from a diarization config
    pub fn from_diarization_config(config: &DiarizationConfig) -> Self {
        Self {
            similarity_threshold: config.similarity_threshold,
            min_similarity: config.min_similarity,
            max_speakers: config.max_speakers,
            ..Default::default()
        }
    }
}

/// Configuration for mel spectrogram generation
#[derive(Debug, Clone)]
pub struct MelConfig {
    /// Sample rate of input audio (must be 16000 for ECAPA-TDNN)
    pub sample_rate: u32,

    /// FFT size
    pub n_fft: usize,

    /// Hop length between frames (in samples)
    pub hop_length: usize,

    /// Window length (in samples)
    pub win_length: usize,

    /// Number of mel frequency bands
    pub n_mels: usize,

    /// Minimum frequency for mel filterbank (Hz)
    pub fmin: f32,

    /// Maximum frequency for mel filterbank (Hz)
    pub fmax: f32,

    /// Small value added before log for numerical stability
    pub log_offset: f32,
}

impl Default for MelConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            n_fft: 512,
            hop_length: 160,  // 10ms at 16kHz
            win_length: 400,  // 25ms at 16kHz
            n_mels: 80,
            fmin: 20.0,
            fmax: 7600.0,
            log_offset: 1e-6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_diarization_config() {
        let config = DiarizationConfig::default();
        assert_eq!(config.similarity_threshold, 0.3);
        assert_eq!(config.max_speakers, 10);
        assert_eq!(config.min_audio_samples, 8000);
    }

    #[test]
    fn test_cluster_config_from_diarization() {
        let diar_config = DiarizationConfig {
            similarity_threshold: 0.8,
            max_speakers: 5,
            ..Default::default()
        };
        let cluster_config = ClusterConfig::from_diarization_config(&diar_config);
        assert_eq!(cluster_config.similarity_threshold, 0.8);
        assert_eq!(cluster_config.max_speakers, 5);
    }

    #[test]
    fn test_mel_config_defaults() {
        let config = MelConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.n_mels, 80);
        assert_eq!(config.hop_length, 160);
        assert_eq!(config.win_length, 400);
    }
}
