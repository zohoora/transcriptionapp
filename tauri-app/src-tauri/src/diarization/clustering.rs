//! Online incremental speaker clustering.
//!
//! Assigns speaker embeddings to clusters in real-time using
//! cosine similarity with exponential moving average centroid updates.

use super::config::ClusterConfig;
use super::{cosine_similarity, l2_normalize};

/// A speaker centroid with running statistics
#[derive(Debug, Clone)]
pub struct SpeakerCentroid {
    /// Unique speaker identifier (e.g., "Speaker 1")
    pub id: String,

    /// Running mean of embeddings (L2 normalized)
    pub centroid: Vec<f32>,

    /// Number of embeddings contributing to this centroid
    pub count: u32,

    /// Timestamp of last update (for potential recency weighting)
    pub last_seen_ms: u64,
}

impl SpeakerCentroid {
    /// Create a new speaker centroid from an initial embedding
    pub fn new(id: String, embedding: Vec<f32>, timestamp_ms: u64) -> Self {
        let mut centroid = embedding;
        l2_normalize(&mut centroid);

        Self {
            id,
            centroid,
            count: 1,
            last_seen_ms: timestamp_ms,
        }
    }

    /// Update centroid with a new embedding using EMA
    pub fn update(&mut self, embedding: &[f32], timestamp_ms: u64, alpha: f32) {
        // Compute the update weight
        // For early embeddings, use simple averaging for stability
        // For stable centroids, use EMA
        let effective_alpha = if self.count < 3 {
            1.0 / (self.count + 1) as f32
        } else {
            alpha
        };

        // Update: centroid = (1 - alpha) * centroid + alpha * embedding
        for (c, e) in self.centroid.iter_mut().zip(embedding.iter()) {
            *c = (1.0 - effective_alpha) * *c + effective_alpha * *e;
        }

        // Re-normalize to unit length
        l2_normalize(&mut self.centroid);

        self.count += 1;
        self.last_seen_ms = timestamp_ms;
    }

    /// Compute cosine similarity to another embedding
    pub fn similarity(&self, embedding: &[f32]) -> f32 {
        cosine_similarity(&self.centroid, embedding)
    }
}

/// Online speaker clustering with incremental centroid updates
#[derive(Debug)]
pub struct SpeakerClusterer {
    /// Current speaker centroids
    centroids: Vec<SpeakerCentroid>,

    /// Configuration parameters
    config: ClusterConfig,

    /// Next speaker number for ID generation
    next_speaker_num: u32,
}

impl SpeakerClusterer {
    /// Create a new speaker clusterer with the given configuration
    pub fn new(config: ClusterConfig) -> Self {
        Self {
            centroids: Vec::new(),
            config,
            next_speaker_num: 0,
        }
    }

    /// Assign a speaker ID to an embedding
    ///
    /// # Arguments
    /// * `embedding` - Speaker embedding (will be L2 normalized internally)
    /// * `timestamp_ms` - Current timestamp in milliseconds
    ///
    /// # Returns
    /// Speaker ID string (e.g., "Speaker 1")
    pub fn assign(&mut self, embedding: &[f32], timestamp_ms: u64) -> String {
        // L2 normalize the embedding
        let mut normalized = embedding.to_vec();
        l2_normalize(&mut normalized);

        // Find best matching centroid
        let best_match = self
            .centroids
            .iter()
            .enumerate()
            .map(|(idx, c)| (idx, c.similarity(&normalized)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        match best_match {
            Some((idx, sim)) if sim >= self.config.similarity_threshold => {
                // Good match found - update existing centroid
                let centroid = &mut self.centroids[idx];
                centroid.update(&normalized, timestamp_ms, self.config.centroid_ema_alpha);
                centroid.id.clone()
            }
            Some((idx, _)) if self.centroids.len() >= self.config.max_speakers => {
                // At max speakers - force merge into closest centroid
                let centroid = &mut self.centroids[idx];
                centroid.update(&normalized, timestamp_ms, self.config.centroid_ema_alpha);
                centroid.id.clone()
            }
            _ => {
                // No good match - create new speaker
                self.create_speaker(normalized, timestamp_ms)
            }
        }
    }

    /// Create a new speaker from an embedding
    fn create_speaker(&mut self, embedding: Vec<f32>, timestamp_ms: u64) -> String {
        self.next_speaker_num += 1;
        let id = format!("Speaker {}", self.next_speaker_num);

        self.centroids
            .push(SpeakerCentroid::new(id.clone(), embedding, timestamp_ms));

        tracing::debug!(
            "Created new speaker: {} (total: {})",
            id,
            self.centroids.len()
        );

        id
    }

    /// Reset the clusterer, clearing all speakers
    pub fn reset(&mut self) {
        self.centroids.clear();
        self.next_speaker_num = 0;
        tracing::debug!("Speaker clusterer reset");
    }

    /// Get the current number of tracked speakers
    pub fn speaker_count(&self) -> usize {
        self.centroids.len()
    }

    /// Get all current speaker IDs
    pub fn speaker_ids(&self) -> Vec<String> {
        self.centroids.iter().map(|c| c.id.clone()).collect()
    }

    /// Get statistics about a specific speaker
    pub fn speaker_stats(&self, speaker_id: &str) -> Option<(u32, u64)> {
        self.centroids
            .iter()
            .find(|c| c.id == speaker_id)
            .map(|c| (c.count, c.last_seen_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ClusterConfig {
        ClusterConfig {
            similarity_threshold: 0.75,
            min_similarity: 0.5,
            max_speakers: 5,
            centroid_ema_alpha: 0.3,
            min_embeddings_stable: 3,
        }
    }

    fn create_normalized_embedding(seed: usize, dim: usize) -> Vec<f32> {
        let mut v: Vec<f32> = (0..dim)
            .map(|i| ((i + seed) as f32).sin())
            .collect();
        l2_normalize(&mut v);
        v
    }

    fn create_orthogonal_embedding(index: usize, dim: usize) -> Vec<f32> {
        let mut v = vec![0.0; dim];
        if index < dim {
            v[index] = 1.0;
        }
        v
    }

    #[test]
    fn test_speaker_centroid_new() {
        let embedding = vec![3.0, 4.0];
        let centroid = SpeakerCentroid::new("Speaker 1".to_string(), embedding, 0);

        assert_eq!(centroid.id, "Speaker 1");
        assert_eq!(centroid.count, 1);

        // Should be L2 normalized
        let norm: f32 = centroid.centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_speaker_centroid_update() {
        let mut centroid = SpeakerCentroid::new("Speaker 1".to_string(), vec![1.0, 0.0], 0);

        // Update with a slightly different embedding
        let mut new_embedding = vec![0.9, 0.1];
        l2_normalize(&mut new_embedding);

        centroid.update(&new_embedding, 1000, 0.3);

        assert_eq!(centroid.count, 2);
        assert_eq!(centroid.last_seen_ms, 1000);

        // Centroid should still be normalized
        let norm: f32 = centroid.centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_clusterer_creates_new_speaker() {
        let config = create_test_config();
        let mut clusterer = SpeakerClusterer::new(config);

        let embedding = create_normalized_embedding(0, 192);
        let speaker = clusterer.assign(&embedding, 0);

        assert_eq!(speaker, "Speaker 1");
        assert_eq!(clusterer.speaker_count(), 1);
    }

    #[test]
    fn test_clusterer_assigns_same_speaker() {
        let config = create_test_config();
        let mut clusterer = SpeakerClusterer::new(config);

        // Create first embedding
        let embedding1 = create_normalized_embedding(0, 192);
        let speaker1 = clusterer.assign(&embedding1, 0);

        // Create very similar embedding
        let embedding2 = create_normalized_embedding(0, 192);
        let speaker2 = clusterer.assign(&embedding2, 1000);

        // Should be assigned to same speaker
        assert_eq!(speaker1, speaker2);
        assert_eq!(clusterer.speaker_count(), 1);
    }

    #[test]
    fn test_clusterer_different_speakers() {
        let config = create_test_config();
        let mut clusterer = SpeakerClusterer::new(config);

        // Create orthogonal embeddings (completely different)
        let embedding1 = create_orthogonal_embedding(0, 192);
        let embedding2 = create_orthogonal_embedding(100, 192);

        let speaker1 = clusterer.assign(&embedding1, 0);
        let speaker2 = clusterer.assign(&embedding2, 1000);

        // Should be different speakers
        assert_ne!(speaker1, speaker2);
        assert_eq!(clusterer.speaker_count(), 2);
    }

    #[test]
    fn test_clusterer_max_speakers_limit() {
        let mut config = create_test_config();
        config.max_speakers = 3;
        let mut clusterer = SpeakerClusterer::new(config);

        // Create 5 completely different embeddings
        for i in 0..5 {
            let embedding = create_orthogonal_embedding(i * 30, 192);
            clusterer.assign(&embedding, i as u64 * 1000);
        }

        // Should cap at max_speakers
        assert!(
            clusterer.speaker_count() <= 3,
            "Should not exceed max_speakers"
        );
    }

    #[test]
    fn test_clusterer_reset() {
        let config = create_test_config();
        let mut clusterer = SpeakerClusterer::new(config);

        // Add some speakers
        for i in 0..3 {
            let embedding = create_orthogonal_embedding(i * 50, 192);
            clusterer.assign(&embedding, i as u64 * 1000);
        }

        assert!(clusterer.speaker_count() > 0);

        // Reset
        clusterer.reset();

        assert_eq!(clusterer.speaker_count(), 0);

        // New speaker should be "Speaker 1" again
        let embedding = create_normalized_embedding(0, 192);
        let speaker = clusterer.assign(&embedding, 0);
        assert_eq!(speaker, "Speaker 1");
    }

    #[test]
    fn test_clusterer_speaker_stats() {
        let config = create_test_config();
        let mut clusterer = SpeakerClusterer::new(config);

        let embedding = create_normalized_embedding(0, 192);
        clusterer.assign(&embedding, 100);
        clusterer.assign(&embedding, 200);
        clusterer.assign(&embedding, 300);

        let stats = clusterer.speaker_stats("Speaker 1");
        assert!(stats.is_some());

        let (count, last_seen) = stats.unwrap();
        assert_eq!(count, 3);
        assert_eq!(last_seen, 300);
    }

    #[test]
    fn test_clusterer_speaker_ids() {
        let config = create_test_config();
        let mut clusterer = SpeakerClusterer::new(config);

        // Create 3 different speakers
        for i in 0..3 {
            let embedding = create_orthogonal_embedding(i * 50, 192);
            clusterer.assign(&embedding, i as u64 * 1000);
        }

        let ids = clusterer.speaker_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"Speaker 1".to_string()));
        assert!(ids.contains(&"Speaker 2".to_string()));
        assert!(ids.contains(&"Speaker 3".to_string()));
    }
}
