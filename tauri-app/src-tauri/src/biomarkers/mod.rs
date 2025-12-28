//! Biomarker Analysis Module
//!
//! Provides real-time vocal biomarker extraction running in parallel with transcription.
//!
//! ## Components
//!
//! - **YAMNet cough detection** - Continuous analysis of ALL audio (including silence)
//! - **Vitality metric** - Pitch variability (F0 std dev) for prosody/emotional engagement
//! - **Stability metric** - CPP (Cepstral Peak Prominence) for neurological control
//! - **Session metrics** - Turn-taking, talk time ratios from diarization data
//!
//! ## Architecture
//!
//! The biomarker thread runs as a sidecar to the main transcription pipeline:
//!
//! ```text
//! 16kHz Resampled Audio
//!          |
//!    [CLONE POINT 1] ──────────────────────┐
//!          |                                |
//!          v                                v
//!     VAD Pipeline                  Biomarker Thread
//!          |                                |
//!          v                        YAMNet (1s windows)
//!     Utterance                             |
//!          |                                v
//!    [CLONE POINT 2] ────────────> Vitality/Stability
//!          |                                |
//!          v                                v
//!   GTCRN Enhancement              Session Aggregator
//!          |                                |
//!          v                                v
//!    Whisper + Diar               VocalBiomarkers
//!          |                                |
//!          v                                |
//!       Segment <───────────────────────────┘
//! ```

pub mod config;
pub mod thread;
pub mod voice_metrics;
#[cfg(feature = "biomarkers")]
pub mod yamnet;
pub mod session_metrics;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub use config::BiomarkerConfig;
pub use thread::{BiomarkerHandle, start_biomarker_thread};

/// Input message types for the biomarker thread
#[derive(Debug, Clone)]
pub enum BiomarkerInput {
    /// Continuous audio chunk for YAMNet (after resample, before VAD)
    AudioChunk {
        samples: Vec<f32>,
        timestamp_ms: u64,
    },
    /// Complete utterance for vitality/stability analysis
    Utterance {
        id: Uuid,
        samples: Vec<f32>,
        start_ms: u64,
        end_ms: u64,
    },
    /// Segment info for session metrics (speaker, duration)
    SegmentInfo {
        speaker_id: Option<String>,
        start_ms: u64,
        end_ms: u64,
    },
    /// Shutdown signal
    Shutdown,
}

/// Output message types from the biomarker thread
#[derive(Debug, Clone)]
pub enum BiomarkerOutput {
    /// Cough detected
    CoughEvent(CoughEvent),
    /// Per-utterance vocal biomarkers ready
    VocalBiomarkers(VocalBiomarkers),
    /// Session metrics update
    SessionMetrics(SessionMetrics),
}

/// Cough detection event from YAMNet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoughEvent {
    pub timestamp_ms: u64,
    pub duration_ms: u32,
    pub confidence: f32,
    pub label: String, // "Cough", "Throat clearing"
}

/// Per-utterance vocal biomarkers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocalBiomarkers {
    pub utterance_id: Uuid,
    pub start_ms: u64,
    pub end_ms: u64,
    /// Vitality: F0 standard deviation in Hz (higher = more pitch variation)
    pub vitality: Option<f32>,
    /// Mean pitch for reference (Hz)
    pub f0_mean: Option<f32>,
    /// Percentage of frames with valid pitch detection
    pub voiced_frame_ratio: f32,
    /// Stability: CPP in dB (higher = more stable/regular voice)
    pub stability: Option<f32>,
}

impl Default for VocalBiomarkers {
    fn default() -> Self {
        Self {
            utterance_id: Uuid::nil(),
            start_ms: 0,
            end_ms: 0,
            vitality: None,
            f0_mean: None,
            voiced_frame_ratio: 0.0,
            stability: None,
        }
    }
}

/// Aggregated session-level metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Total cough count in session
    pub cough_count: u32,
    /// Coughs per minute
    pub cough_rate_per_min: f32,
    /// Talk time per speaker (ms)
    pub speaker_talk_time: HashMap<String, u64>,
    /// Number of speaker turns
    pub turn_count: u32,
    /// Average turn duration (ms)
    pub avg_turn_duration_ms: f32,
    /// Talk time ratio (patient / clinician, if 2 speakers)
    pub talk_time_ratio: Option<f32>,
    /// Session mean vitality
    pub vitality_session_mean: Option<f32>,
    /// Session mean stability
    pub stability_session_mean: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vocal_biomarkers_default() {
        let bio = VocalBiomarkers::default();
        assert!(bio.utterance_id.is_nil());
        assert!(bio.vitality.is_none());
        assert!(bio.stability.is_none());
    }

    #[test]
    fn test_session_metrics_default() {
        let metrics = SessionMetrics::default();
        assert_eq!(metrics.cough_count, 0);
        assert_eq!(metrics.turn_count, 0);
    }
}
