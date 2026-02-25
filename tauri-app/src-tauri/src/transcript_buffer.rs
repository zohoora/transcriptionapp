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
    /// Wall-clock time of the segment (pipeline audio clock)
    pub timestamp_ms: u64,
    /// Absolute time when segment was received
    pub started_at: DateTime<Utc>,
    /// Transcribed text
    pub text: String,
    /// Speaker ID from diarization
    pub speaker_id: Option<String>,
    /// Pipeline generation that produced this segment (prevents stale data across restarts)
    pub generation: u64,
}

/// Safety cap: discard oldest segments when buffer exceeds this count.
/// ~5000 segments = 8 hours at ~10 segments/minute. Prevents unbounded growth
/// if encounter detection fails or is misconfigured.
pub const MAX_BUFFER_SEGMENTS: usize = 5000;

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
    pub fn push(&mut self, text: String, timestamp_ms: u64, speaker_id: Option<String>, generation: u64) {
        if generation < self.current_generation {
            return; // Stale segment from a previous pipeline instance
        }
        let segment = BufferedSegment {
            index: self.next_index,
            timestamp_ms,
            started_at: Utc::now(),
            text,
            speaker_id,
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

    /// Get full text with speaker labels for display (e.g. "Speaker 1: text\n")
    pub fn full_text_with_speakers(&self) -> String {
        self.segments
            .iter()
            .map(|s| {
                if let Some(ref spk) = s.speaker_id {
                    format!("{}: {}", spk, s.text)
                } else {
                    s.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format segments for the encounter detector prompt (numbered)
    pub fn format_for_detection(&self) -> String {
        self.segments
            .iter()
            .map(|s| {
                let speaker = s
                    .speaker_id
                    .as_deref()
                    .unwrap_or("Unknown");
                format!("[{}] ({}): {}", s.index, speaker, s.text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format segments for LLM detection, truncated to ~3000 words.
    /// Keeps first ~1500 words (encounter start) + last ~1500 words (encounter end)
    /// with a separator, so the small detection model doesn't get overwhelmed.
    pub fn format_for_detection_truncated(&self) -> String {
        const MAX_WORDS: usize = 3000;
        const HALF: usize = MAX_WORDS / 2;

        let lines: Vec<String> = self.segments
            .iter()
            .map(|s| {
                let speaker = s.speaker_id.as_deref().unwrap_or("Unknown");
                format!("[{}] ({}): {}", s.index, speaker, s.text)
            })
            .collect();

        // Count words per line to find truncation points
        let word_counts: Vec<usize> = lines.iter()
            .map(|l| l.split_whitespace().count())
            .collect();
        let total_words: usize = word_counts.iter().sum();

        if total_words <= MAX_WORDS {
            return lines.join("\n");
        }

        // Find the line index where first HALF words end
        let mut head_words = 0;
        let mut head_end = 0;
        for (i, &wc) in word_counts.iter().enumerate() {
            head_words += wc;
            if head_words >= HALF {
                head_end = i + 1;
                break;
            }
        }

        // Find the line index where last HALF words start
        let mut tail_words = 0;
        let mut tail_start = lines.len();
        for (i, &wc) in word_counts.iter().enumerate().rev() {
            tail_words += wc;
            if tail_words >= HALF {
                tail_start = i;
                break;
            }
        }

        // Ensure no overlap
        if tail_start <= head_end {
            return lines.join("\n");
        }

        let skipped = tail_start - head_end;
        let head = lines[..head_end].join("\n");
        let tail = lines[tail_start..].join("\n");
        format!("{}\n\n[... {} segments omitted for brevity ...]\n\n{}", head, skipped, tail)
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
        buffer.push("Hello doctor".to_string(), 1000, Some("Speaker 1".to_string()), 0);
        buffer.push("How are you?".to_string(), 2000, Some("Speaker 2".to_string()), 0);

        assert_eq!(buffer.word_count(), 5);
        assert_eq!(buffer.first_index(), Some(0));
        assert_eq!(buffer.last_index(), Some(1));
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_transcript_buffer_full_text() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello".to_string(), 1000, None, 0);
        buffer.push("World".to_string(), 2000, None, 0);

        assert_eq!(buffer.full_text(), "Hello World");
    }

    #[test]
    fn test_transcript_buffer_drain_through() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("A".to_string(), 1000, None, 0);
        buffer.push("B".to_string(), 2000, None, 0);
        buffer.push("C".to_string(), 3000, None, 0);

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
        buffer.push("First".to_string(), 1000, None, 0);
        buffer.push("Second".to_string(), 2000, None, 0);
        buffer.push("Third".to_string(), 3000, None, 0);

        let text = buffer.get_text_since(0);
        assert_eq!(text, "Second Third");
    }

    #[test]
    fn test_transcript_buffer_format_for_detection() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello".to_string(), 1000, Some("Dr. Smith".to_string()), 0);
        buffer.push("Hi there".to_string(), 2000, None, 0);

        let formatted = buffer.format_for_detection();
        assert!(formatted.contains("[0] (Dr. Smith): Hello"));
        assert!(formatted.contains("[1] (Unknown): Hi there"));
    }

    #[test]
    fn test_transcript_buffer_full_text_with_speakers() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello doctor".to_string(), 1000, Some("Speaker 1".to_string()), 0);
        buffer.push("How are you?".to_string(), 2000, Some("Speaker 2".to_string()), 0);
        buffer.push("ambient noise".to_string(), 3000, None, 0);

        let text = buffer.full_text_with_speakers();
        assert_eq!(text, "Speaker 1: Hello doctor\nSpeaker 2: How are you?\nambient noise");
    }

    #[test]
    fn test_transcript_buffer_stale_generation_rejected() {
        let mut buffer = TranscriptBuffer::new();
        buffer.set_generation(2);
        buffer.push("old".to_string(), 1000, None, 1); // stale
        buffer.push("current".to_string(), 2000, None, 2); // current
        assert_eq!(buffer.word_count(), 1);
        assert_eq!(buffer.full_text(), "current");
    }

    #[test]
    fn test_format_for_detection_truncated_small_buffer() {
        // Under 3000 words -- should return everything, same as format_for_detection
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello doctor".to_string(), 1000, Some("Speaker 1".to_string()), 0);
        buffer.push("How are you today".to_string(), 2000, Some("Speaker 2".to_string()), 0);

        let full = buffer.format_for_detection();
        let truncated = buffer.format_for_detection_truncated();
        assert_eq!(full, truncated);
    }

    #[test]
    fn test_format_for_detection_truncated_large_buffer() {
        // Create a buffer with >3000 words to trigger truncation
        let mut buffer = TranscriptBuffer::new();
        // Each segment: ~50 words, so 80 segments = ~4000 words
        for i in 0..80 {
            let text = (0..50).map(|w| format!("word{}seg{}", w, i)).collect::<Vec<_>>().join(" ");
            buffer.push(text, i * 1000, Some(format!("Speaker {}", i % 3)), 0);
        }

        let total_words = buffer.word_count();
        assert!(total_words > 3000, "Buffer should have >3000 words, got {}", total_words);

        let truncated = buffer.format_for_detection_truncated();
        assert!(truncated.contains("segments omitted for brevity"));

        // Should contain first segment and last segment
        assert!(truncated.contains("[0]"));
        assert!(truncated.contains(&format!("[{}]", 79)));

        // Truncated should be shorter than full
        let full = buffer.format_for_detection();
        assert!(truncated.len() < full.len(), "Truncated ({}) should be shorter than full ({})", truncated.len(), full.len());
    }
}
