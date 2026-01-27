//! Main diarization provider facade.
//!
//! Combines mel spectrogram generation, embedding extraction, and speaker
//! clustering into a single easy-to-use interface.
//!
//! Supports enrolled speaker recognition for known speakers.

use super::clustering::SpeakerClusterer;
use super::config::{ClusterConfig, DiarizationConfig, MelConfig};
use super::embedding::EmbeddingExtractor;
use super::mel::MelSpectrogramGenerator;
use super::DiarizationError;
use crate::speaker_profiles::SpeakerProfile;

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
    /// auto-detected speakers. Enrolled speakers are preserved.
    pub fn reset(&mut self) {
        self.clusterer.reset();
        tracing::info!("Diarization provider reset");
    }

    /// Get the current number of identified speakers (auto-detected only)
    pub fn speaker_count(&self) -> usize {
        self.clusterer.speaker_count()
    }

    /// Get all current speaker IDs (auto-detected only)
    pub fn speaker_ids(&self) -> Vec<String> {
        self.clusterer.speaker_ids()
    }

    /// Check if the diarization model is loaded and ready
    pub fn is_ready(&self) -> bool {
        true // If we got this far, the model is loaded
    }

    // =========================================================================
    // Enrolled Speaker Methods
    // =========================================================================

    /// Load enrolled speakers from profiles
    ///
    /// Call this at the start of a recording session to enable
    /// recognition of known speakers. Enrolled speakers have priority
    /// over auto-detected speakers.
    pub fn load_enrolled_speakers(&mut self, profiles: &[SpeakerProfile]) {
        let enrolled_data: Vec<(String, String, Vec<f32>)> = profiles
            .iter()
            .map(|p| (p.id.clone(), p.name.clone(), p.embedding.clone()))
            .collect();

        self.clusterer.load_enrolled_speakers(&enrolled_data);
        tracing::info!("Loaded {} enrolled speakers into diarization", profiles.len());
    }

    /// Extract a voice embedding from audio samples
    ///
    /// Use this during speaker enrollment to get the embedding
    /// that will be stored in the speaker profile.
    ///
    /// # Arguments
    /// * `audio` - Audio samples at 16kHz mono, should be 5-10 seconds
    ///
    /// # Returns
    /// 256-dimensional embedding vector, or error if extraction fails
    pub fn extract_embedding(&mut self, audio: &[f32]) -> Result<Vec<f32>, DiarizationError> {
        // Check minimum audio length (at least 1 second for reliable embedding)
        let min_samples = 16000; // 1 second at 16kHz
        if audio.len() < min_samples {
            return Err(DiarizationError::InvalidAudio(format!(
                "Audio too short for embedding extraction: {} samples (min: {})",
                audio.len(),
                min_samples
            )));
        }

        // Compute mel spectrogram
        let mel_spec = self.mel_gen.compute(audio)?;

        // Check for silence
        let energy = MelSpectrogramGenerator::compute_energy(&mel_spec);
        if energy < self.config.min_energy_threshold.exp() {
            return Err(DiarizationError::InvalidAudio(
                "Audio is too quiet for reliable embedding extraction".to_string()
            ));
        }

        // Extract embedding
        let embedding = self.extractor.extract(&mel_spec)?;

        tracing::info!(
            "Extracted embedding from {} samples ({:.1}s of audio)",
            audio.len(),
            audio.len() as f32 / 16000.0
        );

        Ok(embedding)
    }

    /// Get the number of enrolled speakers
    pub fn enrolled_speaker_count(&self) -> usize {
        self.clusterer.enrolled_speaker_count()
    }

    /// Get all enrolled speaker names
    pub fn enrolled_speaker_names(&self) -> Vec<String> {
        self.clusterer.enrolled_speaker_names()
    }

    /// Check if a speaker name is enrolled
    pub fn is_speaker_enrolled(&self, name: &str) -> bool {
        self.clusterer.is_enrolled(name)
    }

    /// Get all speaker IDs including enrolled speakers
    pub fn all_speaker_ids(&self) -> Vec<String> {
        self.clusterer.all_speaker_ids()
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

    // Stub methods for enrolled speakers
    pub fn load_enrolled_speakers(&mut self, _profiles: &[SpeakerProfile]) {}

    pub fn extract_embedding(&self, _audio: &[f32]) -> Result<Vec<f32>, DiarizationError> {
        Err(DiarizationError::FeatureNotEnabled)
    }

    pub fn enrolled_speaker_count(&self) -> usize {
        0
    }

    pub fn enrolled_speaker_names(&self) -> Vec<String> {
        Vec::new()
    }

    pub fn is_speaker_enrolled(&self, _name: &str) -> bool {
        false
    }

    pub fn all_speaker_ids(&self) -> Vec<String> {
        Vec::new()
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
