//! Transcript buffer for continuous mode.
//!
//! Accumulates timestamped speech segments and provides methods for
//! the encounter detector to read, format, and drain completed encounters.

use chrono::{DateTime, Utc};
use tracing::warn;

/// A timestamped transcript segment in the continuous buffer
#[derive(Debug, Clone)]
pub struct BufferedSegment {
    /// Monotonic sequence number
    pub index: u64,
    /// Pipeline audio clock: when speech started (milliseconds from recording start)
    pub start_ms: u64,
    /// Pipeline audio clock: when speech ended (milliseconds from recording start)
    pub timestamp_ms: u64,
    /// Absolute time when segment was received
    pub started_at: DateTime<Utc>,
    /// Transcribed text
    pub text: String,
    /// Speaker ID from diarization
    pub speaker_id: Option<String>,
    /// Speaker confidence from diarization (0.0-1.0 cosine similarity)
    pub speaker_confidence: Option<f32>,
    /// Pipeline generation that produced this segment (prevents stale data across restarts)
    pub generation: u64,
}

/// Safety cap: discard oldest segments when buffer exceeds this count.
/// ~5000 segments = 8 hours at ~10 segments/minute. Prevents unbounded growth
/// if encounter detection fails or is misconfigured.
pub const MAX_BUFFER_SEGMENTS: usize = 5000;

/// Format a speaker label with optional confidence percentage.
/// e.g. "Speaker 1 (87%)" or just "Speaker 1" if no confidence, or "Unknown" if no speaker.
pub fn format_speaker_label(speaker_id: Option<&str>, confidence: Option<f32>) -> String {
    match (speaker_id, confidence) {
        (Some(spk), Some(conf)) => format!("{} ({:.0}%)", spk, conf * 100.0),
        (Some(spk), None) => spk.to_string(),
        _ => "Unknown".to_string(),
    }
}

/// Format a slice of drained segments into the rich detection format:
/// `[index] (MM:SS) (Speaker Label): text`
///
/// Standalone version of `TranscriptBuffer::format_for_detection()` that works
/// on already-drained segments (e.g. for multi-patient detection after drain).
pub fn format_segments_for_detection(segments: &[BufferedSegment]) -> String {
    let first_start_ms = segments.first().map(|s| s.start_ms).unwrap_or(0);

    segments
        .iter()
        .map(|s| {
            let elapsed_ms = s.start_ms.saturating_sub(first_start_ms);
            let total_secs = elapsed_ms / 1000;
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;

            let elapsed = if hours > 0 {
                format!("{}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                format!("{:02}:{:02}", minutes, seconds)
            };

            let speaker_label = format_speaker_label(s.speaker_id.as_deref(), s.speaker_confidence);
            format!("[{}] ({}) ({}): {}", s.index, elapsed, speaker_label, s.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Thread-safe transcript buffer for continuous mode.
/// Accumulates segments and allows the encounter detector to drain completed encounters.
pub struct TranscriptBuffer {
    segments: Vec<BufferedSegment>,
    next_index: u64,
    /// Current pipeline generation -- segments from older generations are discarded on push
    current_generation: u64,
}

impl TranscriptBuffer {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            next_index: 0,
            current_generation: 0,
        }
    }

    /// Set the expected pipeline generation. Segments from older generations
    /// that arrive after this call will be discarded.
    pub fn set_generation(&mut self, generation: u64) {
        self.current_generation = generation;
    }

    /// Add a new segment to the buffer, tagged with the given generation.
    /// Segments from stale generations are silently dropped.
    pub fn push(&mut self, text: String, start_ms: u64, timestamp_ms: u64, speaker_id: Option<String>, speaker_confidence: Option<f32>, generation: u64) {
        if generation < self.current_generation {
            return; // Stale segment from a previous pipeline instance
        }
        let segment = BufferedSegment {
            index: self.next_index,
            start_ms,
            timestamp_ms,
            started_at: Utc::now(),
            text,
            speaker_id,
            speaker_confidence,
            generation,
        };
        self.next_index += 1;
        self.segments.push(segment);

        // Safety cap: trim oldest segments to prevent unbounded growth
        if self.segments.len() > MAX_BUFFER_SEGMENTS {
            let excess = self.segments.len() - MAX_BUFFER_SEGMENTS;
            warn!(
                "Transcript buffer exceeded {} segments, discarding {} oldest",
                MAX_BUFFER_SEGMENTS, excess
            );
            self.segments.drain(..excess);
        }
    }

    /// Get all text from segments with index > the given index
    pub fn get_text_since(&self, index: u64) -> String {
        self.segments
            .iter()
            .filter(|s| s.index > index)
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Remove and return all segments with index <= through_index
    pub fn drain_through(&mut self, through_index: u64) -> Vec<BufferedSegment> {
        let (drained, remaining): (Vec<_>, Vec<_>) = self
            .segments
            .drain(..)
            .partition(|s| s.index <= through_index);
        self.segments = remaining;
        drained
    }

    /// Get full text of all buffered segments
    pub fn full_text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Get full text with speaker labels and confidence for display
    pub fn full_text_with_speakers(&self) -> String {
        self.segments
            .iter()
            .map(|s| {
                if s.speaker_id.is_some() {
                    let label = format_speaker_label(s.speaker_id.as_deref(), s.speaker_confidence);
                    format!("{}: {}", label, s.text)
                } else {
                    s.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format segments for the encounter detector prompt (numbered, with elapsed time and speaker confidence)
    pub fn format_for_detection(&self) -> String {
        format_segments_for_detection(&self.segments)
    }

    /// Total word count in the buffer
    pub fn word_count(&self) -> usize {
        self.segments
            .iter()
            .map(|s| s.text.split_whitespace().count())
            .sum()
    }

    /// First segment index, if any
    pub fn first_index(&self) -> Option<u64> {
        self.segments.first().map(|s| s.index)
    }

    /// Last segment index, if any
    pub fn last_index(&self) -> Option<u64> {
        self.segments.last().map(|s| s.index)
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Number of segments currently in the buffer
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Get the timestamp of the first segment
    pub fn first_timestamp(&self) -> Option<DateTime<Utc>> {
        self.segments.first().map(|s| s.started_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcript_buffer_push_and_read() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello doctor".to_string(), 0, 1000, Some("Speaker 1".to_string()), Some(0.87), 0);
        buffer.push("How are you?".to_string(), 0, 2000, Some("Speaker 2".to_string()), Some(0.65), 0);

        assert_eq!(buffer.word_count(), 5);
        assert_eq!(buffer.first_index(), Some(0));
        assert_eq!(buffer.last_index(), Some(1));
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_transcript_buffer_full_text() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello".to_string(), 0, 1000, None, None, 0);
        buffer.push("World".to_string(), 0, 2000, None, None, 0);

        assert_eq!(buffer.full_text(), "Hello World");
    }

    #[test]
    fn test_transcript_buffer_drain_through() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("A".to_string(), 0, 1000, None, None, 0);
        buffer.push("B".to_string(), 0, 2000, None, None, 0);
        buffer.push("C".to_string(), 0, 3000, None, None, 0);

        let drained = buffer.drain_through(1);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].text, "A");
        assert_eq!(drained[1].text, "B");

        // Remaining should only have "C"
        assert_eq!(buffer.full_text(), "C");
        assert_eq!(buffer.first_index(), Some(2));
    }

    #[test]
    fn test_transcript_buffer_get_text_since() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("First".to_string(), 0, 1000, None, None, 0);
        buffer.push("Second".to_string(), 0, 2000, None, None, 0);
        buffer.push("Third".to_string(), 0, 3000, None, None, 0);

        let text = buffer.get_text_since(0);
        assert_eq!(text, "Second Third");
    }

    #[test]
    fn test_transcript_buffer_format_for_detection() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello".to_string(), 0, 1000, Some("Dr. Smith".to_string()), Some(0.92), 0);
        buffer.push("Hi there".to_string(), 0, 2000, None, None, 0);

        let formatted = buffer.format_for_detection();
        assert!(formatted.contains("[0] (00:00) (Dr. Smith (92%)): Hello"));
        assert!(formatted.contains("[1] (00:00) (Unknown): Hi there"));
    }

    #[test]
    fn test_transcript_buffer_format_for_detection_no_confidence() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello".to_string(), 0, 1000, Some("Speaker 1".to_string()), None, 0);

        let formatted = buffer.format_for_detection();
        assert!(formatted.contains("[0] (00:00) (Speaker 1): Hello"));
    }

    #[test]
    fn test_transcript_buffer_full_text_with_speakers() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello doctor".to_string(), 0, 1000, Some("Speaker 1".to_string()), Some(0.87), 0);
        buffer.push("How are you?".to_string(), 0, 2000, Some("Speaker 2".to_string()), None, 0);
        buffer.push("ambient noise".to_string(), 0, 3000, None, None, 0);

        let text = buffer.full_text_with_speakers();
        assert_eq!(text, "Speaker 1 (87%): Hello doctor\nSpeaker 2: How are you?\nambient noise");
    }

    #[test]
    fn test_transcript_buffer_stale_generation_rejected() {
        let mut buffer = TranscriptBuffer::new();
        buffer.set_generation(2);
        buffer.push("old".to_string(), 0, 1000, None, None, 1); // stale
        buffer.push("current".to_string(), 0, 2000, None, None, 2); // current
        assert_eq!(buffer.word_count(), 1);
        assert_eq!(buffer.full_text(), "current");
    }

    #[test]
    fn test_buffered_segment_has_start_ms() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello".to_string(), 500, 1500, None, None, 0);
        let drained = buffer.drain_through(0);
        assert_eq!(drained[0].start_ms, 500);
        assert_eq!(drained[0].timestamp_ms, 1500);
    }

    #[test]
    fn test_format_for_detection_includes_elapsed_time() {
        let mut buffer = TranscriptBuffer::new();
        // Segments at 0s, 8s, 35s, 72s (1:12), 178s (2:58)
        buffer.push("Good afternoon.".to_string(), 0, 500, Some("Speaker 1".to_string()), Some(0.87), 0);
        buffer.push("Check blood pressure.".to_string(), 8_000, 9_000, Some("Speaker 2".to_string()), Some(0.65), 0);
        buffer.push("One forty-two.".to_string(), 35_000, 36_000, Some("Speaker 1".to_string()), Some(0.53), 0);
        buffer.push("Was 151 over 86.".to_string(), 72_000, 73_000, Some("Speaker 2".to_string()), Some(0.50), 0);
        buffer.push("I'll be in shortly.".to_string(), 178_000, 179_000, Some("Speaker 1".to_string()), Some(0.68), 0);

        let formatted = buffer.format_for_detection();
        assert!(formatted.contains("[0] (00:00) (Speaker 1 (87%)): Good afternoon."));
        assert!(formatted.contains("[1] (00:08) (Speaker 2 (65%)): Check blood pressure."));
        assert!(formatted.contains("[2] (00:35) (Speaker 1 (53%)): One forty-two."));
        assert!(formatted.contains("[3] (01:12) (Speaker 2 (50%)): Was 151 over 86."));
        assert!(formatted.contains("[4] (02:58) (Speaker 1 (68%)): I'll be in shortly."));
    }

    #[test]
    fn test_format_for_detection_hour_plus() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Start.".to_string(), 0, 500, Some("Speaker 1".to_string()), Some(0.90), 0);
        // 1 hour, 5 minutes, 30 seconds = 3_930_000 ms
        buffer.push("Still here.".to_string(), 3_930_000, 3_931_000, Some("Speaker 1".to_string()), Some(0.85), 0);

        let formatted = buffer.format_for_detection();
        assert!(formatted.contains("[0] (00:00) (Speaker 1 (90%)): Start."));
        assert!(formatted.contains("[1] (1:05:30) (Speaker 1 (85%)): Still here."));
    }

    #[test]
    fn test_format_for_detection_empty_buffer() {
        let buffer = TranscriptBuffer::new();
        assert_eq!(buffer.format_for_detection(), "");
    }

}
