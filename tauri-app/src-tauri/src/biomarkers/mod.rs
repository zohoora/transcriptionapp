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

pub mod audio_quality;
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
    /// Audio chunk with VAD state for quality analysis
    AudioChunkWithVad {
        samples: Vec<f32>,
        timestamp_ms: u64,
        is_speech: bool,
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
    /// Record a dropout event (buffer overflow)
    Dropout,
    /// Reset all per-encounter accumulators (triggered on encounter boundary)
    Reset,
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
    /// Audio quality snapshot
    AudioQuality(AudioQualitySnapshot),
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

/// Per-speaker biomarker metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpeakerBiomarkers {
    /// Speaker identifier (e.g., "Speaker 1", "Speaker 2")
    pub speaker_id: String,
    /// Mean vitality (F0 std dev in Hz) for this speaker
    pub vitality_mean: Option<f32>,
    /// Mean stability (CPP in dB) for this speaker
    pub stability_mean: Option<f32>,
    /// Number of utterances analyzed
    pub utterance_count: u32,
    /// Total talk time in ms
    pub talk_time_ms: u64,
    /// Number of turns for this speaker
    pub turn_count: u32,
    /// Mean turn duration in ms
    pub mean_turn_duration_ms: f32,
    /// Median turn duration in ms
    pub median_turn_duration_ms: f32,
    /// Whether this speaker is an enrolled clinician (Physician/PA/RN/MA)
    #[serde(default)]
    pub is_clinician: bool,
}

/// Per-speaker turn statistics for conversation dynamics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpeakerTurnStats {
    /// Speaker identifier
    pub speaker_id: String,
    /// Number of turns
    pub turn_count: u32,
    /// Mean turn duration in ms
    pub mean_turn_duration_ms: f32,
    /// Median turn duration in ms
    pub median_turn_duration_ms: f32,
}

/// Silence/pause statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SilenceStats {
    /// Total silence time in ms (gaps between speakers)
    pub total_silence_ms: u64,
    /// Number of long pauses (> 2 seconds)
    pub long_pause_count: u32,
    /// Mean pause duration in ms
    pub mean_pause_duration_ms: f32,
    /// Silence ratio (silence / session duration)
    pub silence_ratio: f32,
}

/// Conversation dynamics summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationDynamics {
    /// Per-speaker turn statistics
    pub speaker_turns: Vec<SpeakerTurnStats>,
    /// Silence/pause statistics
    pub silence: SilenceStats,
    /// Total overlap count (speaker B starts before speaker A ends)
    pub total_overlap_count: u32,
    /// Total interruption count (overlap > 500ms)
    pub total_interruption_count: u32,
    /// Mean response latency in ms (time from speaker A ending to speaker B starting)
    pub mean_response_latency_ms: f32,
    /// Engagement score (0-100, heuristic combining balance, response speed, turn frequency)
    pub engagement_score: Option<f32>,
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
    /// Session mean vitality (all speakers combined)
    pub vitality_session_mean: Option<f32>,
    /// Session mean stability (all speakers combined)
    pub stability_session_mean: Option<f32>,
    /// Per-speaker biomarker metrics
    pub speaker_biomarkers: HashMap<String, SpeakerBiomarkers>,
    /// Conversation dynamics (overlaps, interruptions, response latency, silence)
    pub conversation_dynamics: Option<ConversationDynamics>,
}

/// Frontend-friendly biomarker update payload
/// Sent via `biomarker_update` event to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomarkerUpdate {
    /// Total cough count in session
    pub cough_count: u32,
    /// Coughs per minute
    pub cough_rate_per_min: f32,
    /// Number of speaker turns
    pub turn_count: u32,
    /// Average turn duration (ms)
    pub avg_turn_duration_ms: f32,
    /// Talk time ratio (patient / clinician, if 2 speakers)
    pub talk_time_ratio: Option<f32>,
    /// Session mean vitality (F0 std dev in Hz) - all speakers combined
    pub vitality_session_mean: Option<f32>,
    /// Session mean stability (CPP in dB) - all speakers combined
    pub stability_session_mean: Option<f32>,
    /// Per-speaker biomarker metrics
    pub speaker_metrics: Vec<SpeakerBiomarkers>,
    /// Recent audio events (last 10)
    pub recent_events: Vec<CoughEvent>,
    /// Conversation dynamics (overlaps, interruptions, response latency, silence)
    pub conversation_dynamics: Option<ConversationDynamics>,
}

impl BiomarkerUpdate {
    /// Create a new BiomarkerUpdate from SessionMetrics and recent events
    pub fn from_metrics(metrics: &SessionMetrics, recent_events: &[CoughEvent]) -> Self {
        // Convert HashMap to Vec for frontend
        let speaker_metrics: Vec<SpeakerBiomarkers> = metrics
            .speaker_biomarkers
            .values()
            .cloned()
            .collect();

        Self {
            cough_count: metrics.cough_count,
            cough_rate_per_min: metrics.cough_rate_per_min,
            turn_count: metrics.turn_count,
            avg_turn_duration_ms: metrics.avg_turn_duration_ms,
            talk_time_ratio: metrics.talk_time_ratio,
            vitality_session_mean: metrics.vitality_session_mean,
            stability_session_mean: metrics.stability_session_mean,
            speaker_metrics,
            recent_events: recent_events.to_vec(),
            conversation_dynamics: metrics.conversation_dynamics.clone(),
        }
    }
}

/// Audio quality snapshot - emitted periodically during recording
/// Provides real-time metrics for predicting transcript reliability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioQualitySnapshot {
    /// Timestamp in milliseconds since session start
    pub timestamp_ms: u64,

    // Tier 1 - Level metrics
    /// Peak level in dBFS (0 = max, -60 = very quiet)
    pub peak_db: f32,
    /// RMS level in dBFS (average loudness)
    pub rms_db: f32,
    /// Number of clipped samples this period
    pub clipped_samples: u32,
    /// Ratio of clipped samples (0.0-1.0)
    pub clipped_ratio: f32,

    // Tier 2 - SNR metrics
    /// Estimated noise floor in dBFS
    pub noise_floor_db: f32,
    /// Signal-to-noise ratio in dB
    pub snr_db: f32,
    /// Fraction of time with no speech detected
    pub silence_ratio: f32,

    // Counters
    /// Number of buffer dropout events since session start
    pub dropout_count: u32,
    /// Total clipped samples since session start
    pub total_clipped: u32,
    /// Total samples processed
    pub total_samples: u64,
}

/// Audio quality flags - derived from snapshot with thresholds applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioQualityFlags {
    /// Level is in acceptable range (-40 to -6 dBFS)
    pub level_ok: bool,
    /// Clipping is below threshold (< 0.1%)
    pub clipping_ok: bool,
    /// SNR is acceptable (> 10 dB)
    pub snr_ok: bool,
    /// No dropout events
    pub dropout_ok: bool,
    /// Overall quality score (0.0-1.0)
    pub overall_quality: f32,
}

impl From<audio_quality::AudioQualitySnapshot> for AudioQualitySnapshot {
    fn from(snap: audio_quality::AudioQualitySnapshot) -> Self {
        Self {
            timestamp_ms: snap.timestamp_ms,
            peak_db: snap.peak_db,
            rms_db: snap.rms_db,
            clipped_samples: snap.clipped_samples,
            clipped_ratio: snap.clipped_ratio,
            noise_floor_db: snap.noise_floor_db,
            snr_db: snap.snr_db,
            silence_ratio: snap.silence_ratio,
            dropout_count: snap.dropout_count,
            total_clipped: snap.total_clipped,
            total_samples: snap.total_samples,
        }
    }
}

impl From<audio_quality::AudioQualityFlags> for AudioQualityFlags {
    fn from(flags: audio_quality::AudioQualityFlags) -> Self {
        Self {
            level_ok: flags.level_ok,
            clipping_ok: flags.clipping_ok,
            snr_ok: flags.snr_ok,
            dropout_ok: flags.dropout_ok,
            overall_quality: flags.overall_quality,
        }
    }
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
