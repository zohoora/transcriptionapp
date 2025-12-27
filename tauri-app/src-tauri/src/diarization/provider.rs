//! Main diarization provider facade.
//!
//! Combines mel spectrogram generation, embedding extraction, and speaker
//! clustering into a single easy-to-use interface.

use super::clustering::SpeakerClusterer;
use super::config::{ClusterConfig, DiarizationConfig, MelConfig};
use super::embedding::EmbeddingExtractor;
use super::mel::MelSpectrogramGenerator;
use super::DiarizationError;

/// Utterance structure for diarization input
/// (matches the structure from the transcription module)
pub struct Utterance<'a> {
    /// Audio samples at 16kHz mono
    pub audio: &'a [f32],
    /// Start time in milliseconds
    pub start_ms: u64,
    /// End time in milliseconds
    pub end_ms: u64,
}

impl<'a> Utterance<'a> {
    /// Create a new utterance
    pub fn new(audio: &'a [f32], start_ms: u64, end_ms: u64) -> Self {
        Self {
            audio,
            start_ms,
            end_ms,
        }
    }

    /// Get the duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// Main diarization provider that orchestrates the full pipeline
#[cfg(feature = "diarization")]
pub struct DiarizationProvider {
    mel_gen: MelSpectrogramGenerator,
    extractor: EmbeddingExtractor,
    clusterer: SpeakerClusterer,
    config: DiarizationConfig,
}

#[cfg(feature = "diarization")]
impl DiarizationProvider {
    /// Create a new diarization provider
    ///
    /// # Arguments
    /// * `config` - Diarization configuration including model path
    ///
    /// # Returns
    /// Initialized provider or error if model loading fails
    pub fn new(config: DiarizationConfig) -> Result<Self, DiarizationError> {
        // Initialize mel spectrogram generator
        let mel_config = MelConfig::default();
        let mel_gen = MelSpectrogramGenerator::new(mel_config)?;

        // Initialize embedding extractor
        let extractor = EmbeddingExtractor::new(&config.model_path, config.n_threads)?;

        // Initialize clusterer
        let cluster_config = ClusterConfig::from_diarization_config(&config);
        let clusterer = SpeakerClusterer::new(cluster_config);

        tracing::info!(
            "Diarization provider initialized with model: {:?}",
            config.model_path
        );

        Ok(Self {
            mel_gen,
            extractor,
            clusterer,
            config,
        })
    }

    /// Identify the speaker for an utterance
    ///
    /// # Arguments
    /// * `utterance` - Audio utterance with samples and timestamps
    ///
    /// # Returns
    /// Tuple of (Speaker ID, confidence 0.0-1.0) or ("Unknown", 0.0) for edge cases
    pub fn identify_speaker(&mut self, utterance: &Utterance) -> Result<(String, f32), DiarizationError> {
        // Check minimum audio length
        if utterance.audio.len() < self.config.min_audio_samples {
            tracing::debug!(
                "Audio too short for reliable diarization: {} samples (min: {})",
                utterance.audio.len(),
                self.config.min_audio_samples
            );
            return Ok(("Unknown".to_string(), 0.0));
        }

        // Compute mel spectrogram
        let mel_spec = self.mel_gen.compute(utterance.audio)?;

        // Check for silence
        let energy = MelSpectrogramGenerator::compute_energy(&mel_spec);
        if energy < self.config.min_energy_threshold.exp() {
            tracing::debug!(
                "Low energy audio ({}), skipping diarization",
                energy
            );
            return Ok(("Unknown".to_string(), 0.0));
        }

        // Extract embedding
        let embedding = self.extractor.extract(&mel_spec)?;

        // Assign to speaker cluster
        let (speaker_id, confidence) = self.clusterer.assign(&embedding, utterance.start_ms);

        tracing::debug!(
            "Utterance {}ms-{}ms assigned to {} (confidence: {:.1}%)",
            utterance.start_ms,
            utterance.end_ms,
            speaker_id,
            confidence * 100.0
        );

        Ok((speaker_id, confidence))
    }

    /// Identify speaker from raw audio samples
    ///
    /// Convenience method that creates an Utterance internally.
    pub fn identify_speaker_from_audio(
        &mut self,
        audio: &[f32],
        start_ms: u64,
        end_ms: u64,
    ) -> Result<(String, f32), DiarizationError> {
        let utterance = Utterance::new(audio, start_ms, end_ms);
        self.identify_speaker(&utterance)
    }

    /// Reset the speaker clusterer for a new session
    ///
    /// Call this when starting a new recording session to clear
    /// all existing speaker information.
    pub fn reset(&mut self) {
        self.clusterer.reset();
        tracing::info!("Diarization provider reset");
    }

    /// Get the current number of identified speakers
    pub fn speaker_count(&self) -> usize {
        self.clusterer.speaker_count()
    }

    /// Get all current speaker IDs
    pub fn speaker_ids(&self) -> Vec<String> {
        self.clusterer.speaker_ids()
    }

    /// Check if the diarization model is loaded and ready
    pub fn is_ready(&self) -> bool {
        true // If we got this far, the model is loaded
    }
}

// Stub implementation when feature is not enabled
#[cfg(not(feature = "diarization"))]
pub struct DiarizationProvider;

#[cfg(not(feature = "diarization"))]
impl DiarizationProvider {
    pub fn new(_config: DiarizationConfig) -> Result<Self, DiarizationError> {
        Err(DiarizationError::FeatureNotEnabled)
    }

    pub fn identify_speaker(&mut self, _utterance: &Utterance) -> Result<String, DiarizationError> {
        Err(DiarizationError::FeatureNotEnabled)
    }

    pub fn identify_speaker_from_audio(
        &mut self,
        _audio: &[f32],
        _start_ms: u64,
        _end_ms: u64,
    ) -> Result<String, DiarizationError> {
        Err(DiarizationError::FeatureNotEnabled)
    }

    pub fn reset(&mut self) {}

    pub fn speaker_count(&self) -> usize {
        0
    }

    pub fn speaker_ids(&self) -> Vec<String> {
        Vec::new()
    }

    pub fn is_ready(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utterance_duration() {
        let audio = vec![0.0; 16000];
        let utterance = Utterance::new(&audio, 1000, 3000);

        assert_eq!(utterance.duration_ms(), 2000);
    }

    #[test]
    fn test_utterance_duration_saturating() {
        let audio = vec![0.0; 16000];
        // Edge case: end before start
        let utterance = Utterance::new(&audio, 3000, 1000);

        assert_eq!(utterance.duration_ms(), 0);
    }

    #[cfg(not(feature = "diarization"))]
    #[test]
    fn test_stub_provider() {
        use super::super::config::DiarizationConfig;

        let config = DiarizationConfig::default();
        let result = DiarizationProvider::new(config);

        assert!(matches!(result, Err(DiarizationError::FeatureNotEnabled)));
    }
}
