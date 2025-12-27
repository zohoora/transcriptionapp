# ADR-0006: Online Speaker Diarization

## Status

Accepted

## Context

For clinical ambient scribe use cases, transcripts need to distinguish between different speakers (e.g., doctor vs patient). Traditional diarization approaches:

1. **Offline batch processing** - Requires full recording before analysis
2. **Pre-trained speaker models** - Requires enrolling speakers beforehand
3. **External API services** - Adds latency and privacy concerns

We need a solution that:
- Works in real-time during recording
- Doesn't require speaker enrollment
- Runs locally for privacy
- Integrates with our existing VAD pipeline

## Decision

We implemented **online incremental speaker clustering** using:

1. **Speaker embeddings** - Extract speaker embeddings from audio segments using a pre-trained ONNX model
2. **Cosine similarity matching** - Compare new embeddings against existing speaker centroids
3. **Exponential Moving Average (EMA) centroids** - Update speaker representations incrementally
4. **Configurable thresholds** - Similarity threshold for matching, max speakers limit

Architecture:
```
Audio Segment
      │
      ▼
┌─────────────┐
│  Embedding  │  (ONNX model extracts speaker embedding)
│  Extraction │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  Clustering │  (Match to existing or create new speaker)
│   Engine    │
└──────┬──────┘
       │
       ▼
Speaker ID (e.g., "Speaker 1")
```

Key design choices:
- **EMA with warm-up**: First 3 embeddings use simple averaging for stability, then switch to EMA (alpha=0.3)
- **Max speakers limit**: Force-merge into closest centroid when limit reached
- **L2 normalization**: All embeddings normalized for consistent cosine similarity

Configuration options:
```rust
pub struct ClusterConfig {
    pub similarity_threshold: f32,  // 0.75 default
    pub min_similarity: f32,        // 0.5 floor
    pub max_speakers: usize,        // User configurable (2-10)
    pub centroid_ema_alpha: f32,    // 0.3 default
    pub min_embeddings_stable: u32, // 3 for stable centroid
}
```

## Consequences

### Positive

- Real-time speaker labels during recording
- No speaker enrollment required
- Privacy-preserving (runs locally)
- Configurable max speakers for different scenarios
- Graceful degradation when speakers exceed limit

### Negative

- Embedding model adds ~50MB to app size
- CPU overhead for embedding extraction
- May mis-cluster similar voices initially
- Speaker IDs are session-specific (not persistent)

## References

- [SpeechBrain Speaker Embeddings](https://speechbrain.github.io/)
- [Online Speaker Clustering Survey](https://arxiv.org/abs/2006.06643)
- [ECAPA-TDNN Architecture](https://arxiv.org/abs/2005.07143)
