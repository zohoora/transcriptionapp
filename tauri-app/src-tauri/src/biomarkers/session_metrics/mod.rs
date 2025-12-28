//! Session metrics aggregation
//!
//! Tracks session-level statistics from diarization data:
//! - Cough count and rate
//! - Speaker talk time
//! - Turn count and duration
//! - Talk time ratio (patient vs clinician)

use std::collections::HashMap;
use super::SessionMetrics;

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
            self.last_speaker = Some(speaker);
        }

        self.total_turn_duration_ms += duration;
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

        SessionMetrics {
            cough_count: self.cough_count,
            cough_rate_per_min,
            speaker_talk_time: self.speaker_talk_time.clone(),
            turn_count: self.turn_count,
            avg_turn_duration_ms,
            talk_time_ratio,
            vitality_session_mean: None, // Filled in by caller
            stability_session_mean: None, // Filled in by caller
        }
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
}
