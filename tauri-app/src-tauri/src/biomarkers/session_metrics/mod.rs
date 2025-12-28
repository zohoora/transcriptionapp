//! Session metrics aggregation
//!
//! Tracks session-level statistics from diarization data:
//! - Cough count and rate
//! - Speaker talk time
//! - Turn count and duration
//! - Talk time ratio (patient vs clinician)
//! - Conversation dynamics (overlaps, interruptions, response latency, silence)

use std::collections::{HashMap, VecDeque};
use super::{SessionMetrics, ConversationDynamics, SpeakerTurnStats, SilenceStats};

/// Maximum number of segments to store for history analysis
const MAX_SEGMENT_HISTORY: usize = 100;

/// Threshold in ms for considering an overlap as an interruption
const INTERRUPTION_THRESHOLD_MS: i64 = 500;

/// Threshold in ms for considering a pause as "long"
const LONG_PAUSE_THRESHOLD_MS: u64 = 2000;

/// Stored segment for history analysis
#[derive(Debug, Clone)]
struct StoredSegment {
    speaker_id: String,
    end_ms: u64,
}

/// Aggregates session-level metrics from diarization and biomarker data
pub struct SessionAggregator {
    /// Session start time (first segment timestamp)
    session_start_ms: Option<u64>,
    /// Session end time (last segment end timestamp)
    session_end_ms: u64,
    /// Total coughs detected
    cough_count: u32,
    /// Talk time per speaker in milliseconds
    speaker_talk_time: HashMap<String, u64>,
    /// Number of speaker turns
    turn_count: u32,
    /// Last speaker ID for turn detection
    last_speaker: Option<String>,
    /// Sum of turn durations for averaging
    total_turn_duration_ms: u64,

    // --- New fields for conversation dynamics ---

    /// Bounded segment history for cross-segment analysis
    segment_history: VecDeque<StoredSegment>,
    /// Turn durations per speaker for mean/median calculation
    speaker_turn_durations: HashMap<String, Vec<u64>>,
    /// Silence gaps between different speakers (positive gaps only)
    silence_gaps: Vec<u64>,
    /// Total overlap count (when speaker B starts before speaker A ends)
    total_overlap_count: u32,
    /// Total interruption count (overlap > 500ms)
    total_interruption_count: u32,
    /// Response latencies (positive gaps between different speakers)
    response_latencies: Vec<u64>,
    /// Per-speaker turn counts
    speaker_turn_counts: HashMap<String, u32>,
}

impl SessionAggregator {
    pub fn new() -> Self {
        Self {
            session_start_ms: None,
            session_end_ms: 0,
            cough_count: 0,
            speaker_talk_time: HashMap::new(),
            turn_count: 0,
            last_speaker: None,
            total_turn_duration_ms: 0,
            // New fields
            segment_history: VecDeque::with_capacity(MAX_SEGMENT_HISTORY),
            speaker_turn_durations: HashMap::new(),
            silence_gaps: Vec::new(),
            total_overlap_count: 0,
            total_interruption_count: 0,
            response_latencies: Vec::new(),
            speaker_turn_counts: HashMap::new(),
        }
    }

    /// Add a cough event
    pub fn add_cough(&mut self) {
        self.cough_count += 1;
    }

    /// Add a speaker turn (segment)
    pub fn add_turn(&mut self, speaker_id: Option<&str>, start_ms: u64, end_ms: u64) {
        // Update session timing
        if self.session_start_ms.is_none() {
            self.session_start_ms = Some(start_ms);
        }
        self.session_end_ms = end_ms.max(self.session_end_ms);

        let duration = end_ms.saturating_sub(start_ms);

        // Update speaker talk time
        let speaker = speaker_id.unwrap_or("Unknown").to_string();
        *self.speaker_talk_time.entry(speaker.clone()).or_insert(0) += duration;

        // Check for turn change
        let is_new_turn = match &self.last_speaker {
            Some(last) => last != &speaker,
            None => true,
        };

        if is_new_turn {
            self.turn_count += 1;
            self.last_speaker = Some(speaker.clone());

            // Track per-speaker turn count
            *self.speaker_turn_counts.entry(speaker.clone()).or_insert(0) += 1;
        }

        // Track turn duration per speaker
        self.speaker_turn_durations
            .entry(speaker.clone())
            .or_default()
            .push(duration);

        self.total_turn_duration_ms += duration;

        // --- Conversation dynamics analysis ---

        // Compare with previous segment for overlap/silence detection
        if let Some(prev_segment) = self.segment_history.back() {
            // Only analyze when speakers differ (conversation dynamics)
            if prev_segment.speaker_id != speaker {
                // Calculate gap: positive = silence, negative = overlap
                let gap_ms = start_ms as i64 - prev_segment.end_ms as i64;

                if gap_ms < 0 {
                    // Overlap detected
                    self.total_overlap_count += 1;

                    // Check if it's a significant interruption (> 500ms overlap)
                    if gap_ms.abs() > INTERRUPTION_THRESHOLD_MS {
                        self.total_interruption_count += 1;
                    }
                } else {
                    // Positive gap = response latency / silence
                    let gap_u64 = gap_ms as u64;
                    self.response_latencies.push(gap_u64);

                    // Track as silence gap
                    self.silence_gaps.push(gap_u64);
                }
            }
        }

        // Store segment in history (bounded)
        let segment = StoredSegment {
            speaker_id: speaker,
            end_ms,
        };

        if self.segment_history.len() >= MAX_SEGMENT_HISTORY {
            self.segment_history.pop_front();
        }
        self.segment_history.push_back(segment);
    }

    /// Get current session metrics
    pub fn get_metrics(&self) -> SessionMetrics {
        // Calculate session duration in minutes
        let session_duration_ms = match self.session_start_ms {
            Some(start) => self.session_end_ms.saturating_sub(start),
            None => 0,
        };
        let session_minutes = session_duration_ms as f32 / 60_000.0;

        // Calculate cough rate
        let cough_rate_per_min = if session_minutes > 0.0 {
            self.cough_count as f32 / session_minutes
        } else {
            0.0
        };

        // Calculate average turn duration
        let avg_turn_duration_ms = if self.turn_count > 0 {
            self.total_turn_duration_ms as f32 / self.turn_count as f32
        } else {
            0.0
        };

        // Calculate talk time ratio (if exactly 2 speakers)
        // Convention: smaller talk time / larger talk time
        let talk_time_ratio = if self.speaker_talk_time.len() == 2 {
            let times: Vec<u64> = self.speaker_talk_time.values().cloned().collect();
            let (min, max) = if times[0] < times[1] {
                (times[0], times[1])
            } else {
                (times[1], times[0])
            };
            if max > 0 {
                Some(min as f32 / max as f32)
            } else {
                None
            }
        } else {
            None
        };

        // Build conversation dynamics
        let conversation_dynamics = self.compute_conversation_dynamics(session_duration_ms, talk_time_ratio);

        SessionMetrics {
            cough_count: self.cough_count,
            cough_rate_per_min,
            speaker_talk_time: self.speaker_talk_time.clone(),
            turn_count: self.turn_count,
            avg_turn_duration_ms,
            talk_time_ratio,
            vitality_session_mean: None, // Filled in by caller
            stability_session_mean: None, // Filled in by caller
            speaker_biomarkers: HashMap::new(), // Filled in by caller
            conversation_dynamics: Some(conversation_dynamics),
        }
    }

    /// Compute conversation dynamics metrics
    fn compute_conversation_dynamics(&self, session_duration_ms: u64, talk_time_ratio: Option<f32>) -> ConversationDynamics {
        // Build per-speaker turn stats
        let speaker_turns: Vec<SpeakerTurnStats> = self
            .speaker_turn_durations
            .iter()
            .map(|(speaker_id, durations)| {
                let turn_count = *self.speaker_turn_counts.get(speaker_id).unwrap_or(&0);
                let mean_turn_duration_ms = if !durations.is_empty() {
                    durations.iter().sum::<u64>() as f32 / durations.len() as f32
                } else {
                    0.0
                };
                let median_turn_duration_ms = Self::compute_median(durations);

                SpeakerTurnStats {
                    speaker_id: speaker_id.clone(),
                    turn_count,
                    mean_turn_duration_ms,
                    median_turn_duration_ms,
                }
            })
            .collect();

        // Compute silence statistics
        let total_silence_ms: u64 = self.silence_gaps.iter().sum();
        let long_pause_count = self
            .silence_gaps
            .iter()
            .filter(|&&gap| gap > LONG_PAUSE_THRESHOLD_MS)
            .count() as u32;
        let mean_pause_duration_ms = if !self.silence_gaps.is_empty() {
            total_silence_ms as f32 / self.silence_gaps.len() as f32
        } else {
            0.0
        };
        let silence_ratio = if session_duration_ms > 0 {
            total_silence_ms as f32 / session_duration_ms as f32
        } else {
            0.0
        };

        let silence = SilenceStats {
            total_silence_ms,
            long_pause_count,
            mean_pause_duration_ms,
            silence_ratio,
        };

        // Compute mean response latency
        let mean_response_latency_ms = if !self.response_latencies.is_empty() {
            self.response_latencies.iter().sum::<u64>() as f32
                / self.response_latencies.len() as f32
        } else {
            0.0
        };

        // Compute engagement score (0-100 heuristic)
        let engagement_score = self.compute_engagement_score(
            talk_time_ratio,
            mean_response_latency_ms,
            session_duration_ms,
        );

        ConversationDynamics {
            speaker_turns,
            silence,
            total_overlap_count: self.total_overlap_count,
            total_interruption_count: self.total_interruption_count,
            mean_response_latency_ms,
            engagement_score,
        }
    }

    /// Compute median of a slice of durations
    fn compute_median(durations: &[u64]) -> f32 {
        if durations.is_empty() {
            return 0.0;
        }

        let mut sorted: Vec<u64> = durations.to_vec();
        sorted.sort_unstable();

        let len = sorted.len();
        if len.is_multiple_of(2) {
            // Even number of elements: average of two middle values
            let mid = len / 2;
            (sorted[mid - 1] + sorted[mid]) as f32 / 2.0
        } else {
            // Odd number: middle value
            sorted[len / 2] as f32
        }
    }

    /// Compute engagement score (0-100)
    ///
    /// Heuristic combining:
    /// - Balance: More balanced conversations score higher (if 2 speakers)
    /// - Response speed: Faster responses score higher (up to a point)
    /// - Turn frequency: More turns per minute suggests more engagement
    fn compute_engagement_score(
        &self,
        talk_time_ratio: Option<f32>,
        mean_response_latency_ms: f32,
        session_duration_ms: u64,
    ) -> Option<f32> {
        // Need at least 2 speakers and some session duration
        if self.speaker_talk_time.len() < 2 || session_duration_ms == 0 {
            return None;
        }

        // Balance component (0-40 points)
        // Ratio of 0.5-1.0 is ideal (50% to 100% balance)
        let balance_score = match talk_time_ratio {
            Some(ratio) => {
                // Ratio is min/max, so 0.5 = 33%/67%, 1.0 = 50%/50%
                // Scale: 0.3 or below = 0 points, 1.0 = 40 points
                let normalized = (ratio - 0.3).max(0.0) / 0.7;
                normalized * 40.0
            }
            None => 20.0, // Default if ratio unavailable
        };

        // Response speed component (0-30 points)
        // < 500ms = 30 points, 500-1500ms = linear decay, > 1500ms = 0 points
        let response_score = if mean_response_latency_ms < 500.0 {
            30.0
        } else if mean_response_latency_ms < 1500.0 {
            // Linear decay from 30 to 0
            30.0 * (1.0 - (mean_response_latency_ms - 500.0) / 1000.0)
        } else {
            0.0
        };

        // Turn frequency component (0-30 points)
        // Target: 6-12 turns per minute is ideal engagement
        let session_minutes = session_duration_ms as f32 / 60_000.0;
        let turns_per_minute = if session_minutes > 0.0 {
            self.turn_count as f32 / session_minutes
        } else {
            0.0
        };

        let turn_score = if (6.0..=12.0).contains(&turns_per_minute) {
            30.0
        } else if turns_per_minute < 6.0 {
            // Below 6: linear scale from 0 to 30
            (turns_per_minute / 6.0) * 30.0
        } else {
            // Above 12: decay (too rapid might be interruptions)
            let excess = (turns_per_minute - 12.0).min(12.0);
            30.0 - (excess / 12.0) * 15.0
        };

        Some((balance_score + response_score + turn_score).clamp(0.0, 100.0))
    }

    /// Reset the aggregator for a new session
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.session_start_ms = None;
        self.session_end_ms = 0;
        self.cough_count = 0;
        self.speaker_talk_time.clear();
        self.turn_count = 0;
        self.last_speaker = None;
        self.total_turn_duration_ms = 0;
        // New fields
        self.segment_history.clear();
        self.speaker_turn_durations.clear();
        self.silence_gaps.clear();
        self.total_overlap_count = 0;
        self.total_interruption_count = 0;
        self.response_latencies.clear();
        self.speaker_turn_counts.clear();
    }
}

impl Default for SessionAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_aggregator() {
        let agg = SessionAggregator::new();
        let metrics = agg.get_metrics();
        assert_eq!(metrics.cough_count, 0);
        assert_eq!(metrics.turn_count, 0);
    }

    #[test]
    fn test_add_cough() {
        let mut agg = SessionAggregator::new();
        agg.add_cough();
        agg.add_cough();
        agg.add_cough();

        let metrics = agg.get_metrics();
        assert_eq!(metrics.cough_count, 3);
    }

    #[test]
    fn test_add_turns() {
        let mut agg = SessionAggregator::new();

        // Speaker A speaks
        agg.add_turn(Some("Speaker_A"), 0, 5000);
        // Speaker B responds
        agg.add_turn(Some("Speaker_B"), 5000, 8000);
        // Speaker A again
        agg.add_turn(Some("Speaker_A"), 8000, 10000);

        let metrics = agg.get_metrics();
        assert_eq!(metrics.turn_count, 3);
        assert_eq!(*metrics.speaker_talk_time.get("Speaker_A").unwrap(), 7000);
        assert_eq!(*metrics.speaker_talk_time.get("Speaker_B").unwrap(), 3000);
    }

    #[test]
    fn test_talk_time_ratio() {
        let mut agg = SessionAggregator::new();

        // Speaker A: 6 seconds
        agg.add_turn(Some("Speaker_A"), 0, 6000);
        // Speaker B: 3 seconds
        agg.add_turn(Some("Speaker_B"), 6000, 9000);

        let metrics = agg.get_metrics();
        assert!(metrics.talk_time_ratio.is_some());
        // 3000 / 6000 = 0.5
        assert!((metrics.talk_time_ratio.unwrap() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_no_talk_time_ratio_for_single_speaker() {
        let mut agg = SessionAggregator::new();
        agg.add_turn(Some("Speaker_A"), 0, 5000);
        agg.add_turn(Some("Speaker_A"), 5000, 10000);

        let metrics = agg.get_metrics();
        assert!(metrics.talk_time_ratio.is_none());
    }

    #[test]
    fn test_cough_rate() {
        let mut agg = SessionAggregator::new();

        // 1 minute session
        agg.add_turn(Some("Speaker_A"), 0, 60000);

        // 3 coughs in 1 minute
        agg.add_cough();
        agg.add_cough();
        agg.add_cough();

        let metrics = agg.get_metrics();
        assert!((metrics.cough_rate_per_min - 3.0).abs() < 0.1);
    }

    #[test]
    fn test_consecutive_same_speaker() {
        let mut agg = SessionAggregator::new();

        // Same speaker, should only count as 1 turn
        agg.add_turn(Some("Speaker_A"), 0, 2000);
        agg.add_turn(Some("Speaker_A"), 2000, 4000);
        agg.add_turn(Some("Speaker_A"), 4000, 6000);

        let metrics = agg.get_metrics();
        assert_eq!(metrics.turn_count, 1);
        assert_eq!(*metrics.speaker_talk_time.get("Speaker_A").unwrap(), 6000);
    }

    // --- New tests for conversation dynamics ---

    #[test]
    fn test_overlap_detection() {
        let mut agg = SessionAggregator::new();

        // Speaker A speaks 0-5000ms
        agg.add_turn(Some("Speaker_A"), 0, 5000);
        // Speaker B starts at 4500ms (overlaps by 500ms)
        agg.add_turn(Some("Speaker_B"), 4500, 7000);

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        assert_eq!(dynamics.total_overlap_count, 1);
        // 500ms overlap is exactly at threshold, should NOT be interruption
        assert_eq!(dynamics.total_interruption_count, 0);
    }

    #[test]
    fn test_interruption_detection() {
        let mut agg = SessionAggregator::new();

        // Speaker A speaks 0-5000ms
        agg.add_turn(Some("Speaker_A"), 0, 5000);
        // Speaker B interrupts at 4000ms (overlaps by 1000ms > 500ms threshold)
        agg.add_turn(Some("Speaker_B"), 4000, 7000);

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        assert_eq!(dynamics.total_overlap_count, 1);
        assert_eq!(dynamics.total_interruption_count, 1);
    }

    #[test]
    fn test_response_latency() {
        let mut agg = SessionAggregator::new();

        // Speaker A speaks 0-5000ms
        agg.add_turn(Some("Speaker_A"), 0, 5000);
        // Speaker B responds at 5800ms (800ms gap)
        agg.add_turn(Some("Speaker_B"), 5800, 8000);
        // Speaker A responds at 8500ms (500ms gap)
        agg.add_turn(Some("Speaker_A"), 8500, 10000);

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        // Mean of 800 and 500 = 650
        assert!((dynamics.mean_response_latency_ms - 650.0).abs() < 1.0);
        assert_eq!(dynamics.total_overlap_count, 0);
    }

    #[test]
    fn test_silence_stats() {
        let mut agg = SessionAggregator::new();

        // Speaker A speaks 0-5000ms
        agg.add_turn(Some("Speaker_A"), 0, 5000);
        // Speaker B responds at 6000ms (1000ms gap)
        agg.add_turn(Some("Speaker_B"), 6000, 8000);
        // Speaker A responds at 11000ms (3000ms gap - long pause)
        agg.add_turn(Some("Speaker_A"), 11000, 13000);

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        // Total silence: 1000 + 3000 = 4000ms
        assert_eq!(dynamics.silence.total_silence_ms, 4000);
        // One long pause (3000ms > 2000ms threshold)
        assert_eq!(dynamics.silence.long_pause_count, 1);
        // Mean pause: (1000 + 3000) / 2 = 2000ms
        assert!((dynamics.silence.mean_pause_duration_ms - 2000.0).abs() < 1.0);
    }

    #[test]
    fn test_per_speaker_turn_stats() {
        let mut agg = SessionAggregator::new();

        // Speaker A: 3 turns with durations 2000, 3000, 4000
        agg.add_turn(Some("Speaker_A"), 0, 2000);
        agg.add_turn(Some("Speaker_B"), 2000, 3000);
        agg.add_turn(Some("Speaker_A"), 3000, 6000);
        agg.add_turn(Some("Speaker_B"), 6000, 7500);
        agg.add_turn(Some("Speaker_A"), 7500, 11500);

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        // Find Speaker A stats
        let speaker_a = dynamics
            .speaker_turns
            .iter()
            .find(|s| s.speaker_id == "Speaker_A")
            .unwrap();

        assert_eq!(speaker_a.turn_count, 3);
        // Mean: (2000 + 3000 + 4000) / 3 = 3000
        assert!((speaker_a.mean_turn_duration_ms - 3000.0).abs() < 1.0);
        // Median of [2000, 3000, 4000] = 3000
        assert!((speaker_a.median_turn_duration_ms - 3000.0).abs() < 1.0);
    }

    #[test]
    fn test_median_even_count() {
        let durations = vec![1000, 2000, 3000, 4000];
        let median = SessionAggregator::compute_median(&durations);
        // Median of [1000, 2000, 3000, 4000] = (2000 + 3000) / 2 = 2500
        assert!((median - 2500.0).abs() < 1.0);
    }

    #[test]
    fn test_median_odd_count() {
        let durations = vec![1000, 2000, 3000, 4000, 5000];
        let median = SessionAggregator::compute_median(&durations);
        // Median of [1000, 2000, 3000, 4000, 5000] = 3000
        assert!((median - 3000.0).abs() < 1.0);
    }

    #[test]
    fn test_engagement_score_balanced_conversation() {
        let mut agg = SessionAggregator::new();

        // Create a well-balanced 60-second conversation
        // Speaker A: ~30s, Speaker B: ~30s, fast responses, good turn frequency
        let mut t = 0;
        for i in 0..12 {
            let speaker = if i % 2 == 0 { "Speaker_A" } else { "Speaker_B" };
            let duration = 5000; // 5 second turns
            agg.add_turn(Some(speaker), t, t + duration);
            t += duration + 200; // 200ms response latency
        }

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        // Should have a high engagement score
        assert!(dynamics.engagement_score.is_some());
        let score = dynamics.engagement_score.unwrap();
        // Well-balanced, fast response, good frequency = high score
        assert!(score > 70.0, "Expected high engagement score, got {}", score);
    }

    #[test]
    fn test_engagement_score_imbalanced() {
        let mut agg = SessionAggregator::new();

        // Create an imbalanced conversation
        // Speaker A dominates: 50s, Speaker B: 10s
        agg.add_turn(Some("Speaker_A"), 0, 50000);
        agg.add_turn(Some("Speaker_B"), 50500, 60500);

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        assert!(dynamics.engagement_score.is_some());
        let score = dynamics.engagement_score.unwrap();
        // Imbalanced = lower score (but still some points for other factors)
        assert!(score < 60.0, "Expected lower engagement score for imbalanced conversation, got {}", score);
    }

    #[test]
    fn test_no_engagement_score_single_speaker() {
        let mut agg = SessionAggregator::new();

        // Single speaker monologue
        agg.add_turn(Some("Speaker_A"), 0, 60000);

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        // No engagement score for single speaker
        assert!(dynamics.engagement_score.is_none());
    }

    #[test]
    fn test_same_speaker_no_overlap_count() {
        let mut agg = SessionAggregator::new();

        // Same speaker overlapping segments should not count as overlaps
        agg.add_turn(Some("Speaker_A"), 0, 5000);
        agg.add_turn(Some("Speaker_A"), 4000, 8000); // Same speaker, overlap

        let metrics = agg.get_metrics();
        let dynamics = metrics.conversation_dynamics.unwrap();

        // Same speaker "overlap" doesn't count
        assert_eq!(dynamics.total_overlap_count, 0);
    }

    #[test]
    fn test_conversation_dynamics_present() {
        let mut agg = SessionAggregator::new();
        agg.add_turn(Some("Speaker_A"), 0, 5000);

        let metrics = agg.get_metrics();
        assert!(metrics.conversation_dynamics.is_some());
    }
}
