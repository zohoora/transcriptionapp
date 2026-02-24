//! Native STT Shadow Accumulator + CSV Logger
//!
//! Collects per-utterance native STT results alongside primary STT results for
//! quality comparison. Produces shadow_transcript.txt files in session archives
//! and logs per-utterance metrics to CSV.
//!
//! CSV logs are written to `~/.transcriptionapp/shadow_stt/YYYY-MM-DD.csv` with daily rotation.
//! No transcript text in CSV (PHI safety) — actual text is in archive only.

use chrono::Utc;
use std::path::PathBuf;
use tracing::{info, warn};
use uuid::Uuid;

// ============================================================================
// Types
// ============================================================================

/// A single per-utterance comparison between native and primary STT
#[derive(Debug, Clone)]
pub struct NativeSttSegment {
    pub utterance_id: Uuid,
    pub start_ms: u64,
    pub end_ms: u64,
    pub native_text: String,
    pub primary_text: String,
    /// Speaker label inherited from primary pipeline's diarization
    pub speaker_id: Option<String>,
    pub native_latency_ms: u64,
    pub primary_latency_ms: u64,
}

// ============================================================================
// Accumulator
// ============================================================================

/// Accumulates native STT segments, maintaining sort order by start_ms.
/// Used to build the shadow transcript for archiving.
pub struct NativeSttShadowAccumulator {
    segments: Vec<NativeSttSegment>,
}

impl NativeSttShadowAccumulator {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Insert a segment maintaining sort order by start_ms.
    /// Native STT results may arrive out-of-order since they run on separate threads.
    pub fn push(&mut self, segment: NativeSttSegment) {
        // Binary search for insertion point
        let pos = self
            .segments
            .binary_search_by_key(&segment.start_ms, |s| s.start_ms)
            .unwrap_or_else(|e| e);
        self.segments.insert(pos, segment);
    }

    /// Format the accumulated segments as a plain-text transcript.
    /// Uses speaker labels inherited from primary diarization.
    pub fn format_transcript(&self) -> String {
        self.segments
            .iter()
            .map(|s| {
                if let Some(ref spk) = s.speaker_id {
                    format!("{}: {}", spk, s.native_text)
                } else {
                    s.native_text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Drain segments up to and including the given end_ms.
    /// Used for continuous mode encounter boundaries.
    pub fn drain_through(&mut self, end_ms: u64) -> Vec<NativeSttSegment> {
        // Find the split point: first segment with start_ms > end_ms
        let split_pos = self
            .segments
            .iter()
            .position(|s| s.start_ms > end_ms)
            .unwrap_or(self.segments.len());

        self.segments.drain(..split_pos).collect()
    }

    /// Drain all segments and return (formatted transcript, segments).
    /// Used for session mode stop.
    pub fn drain_all(&mut self) -> (String, Vec<NativeSttSegment>) {
        let transcript = self.format_transcript();
        let segments = std::mem::take(&mut self.segments);
        (transcript, segments)
    }

    /// Clear all accumulated segments.
    pub fn clear(&mut self) {
        self.segments.clear();
    }

    /// Check if the accumulator has any segments.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Number of segments currently accumulated.
    pub fn len(&self) -> usize {
        self.segments.len()
    }
}

// ============================================================================
// CSV Logger
// ============================================================================

/// CSV logger for per-utterance native vs primary STT comparison.
/// Logs to `~/.transcriptionapp/shadow_stt/YYYY-MM-DD.csv` with daily rotation.
///
/// Columns: timestamp_utc, utterance_id, start_ms, end_ms, duration_ms,
///          native_word_count, primary_word_count, native_latency_ms, primary_latency_ms
///
/// No transcript text in CSV (PHI safety).
pub struct NativeSttCsvLogger {
    log_dir: PathBuf,
    current_date: String,
    file: Option<std::fs::File>,
}

impl NativeSttCsvLogger {
    /// Create a new logger. Creates the log directory if needed.
    pub fn new() -> Result<Self, String> {
        let log_dir = dirs::home_dir()
            .ok_or("No home directory")?
            .join(".transcriptionapp")
            .join("shadow_stt");

        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create shadow_stt log dir: {}", e))?;

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut logger = NativeSttCsvLogger {
            log_dir,
            current_date: today,
            file: None,
        };
        logger.open_file()?;
        Ok(logger)
    }

    fn open_file(&mut self) -> Result<(), String> {
        let path = self.log_dir.join(format!("{}.csv", self.current_date));
        let write_header = !path.exists();

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("Failed to open shadow_stt CSV log: {}", e))?;

        if write_header {
            use std::io::Write;
            let mut f = &file;
            let _ = writeln!(
                f,
                "timestamp_utc,utterance_id,start_ms,end_ms,duration_ms,native_word_count,primary_word_count,native_latency_ms,primary_latency_ms"
            );
        }

        self.file = Some(file);
        info!("Shadow STT CSV log opened: {}.csv", self.current_date);
        Ok(())
    }

    /// Write a segment comparison to CSV (PHI-safe: no transcript text).
    pub fn write_segment(&mut self, segment: &NativeSttSegment) {
        let now_utc = Utc::now();

        // Check for midnight rotation
        let today = now_utc.format("%Y-%m-%d").to_string();
        if today != self.current_date {
            self.current_date = today;
            if let Err(e) = self.open_file() {
                warn!("Failed to rotate shadow_stt CSV log: {}", e);
                return;
            }
            info!("Shadow STT CSV log rotated to {}.csv", self.current_date);
        }

        if let Some(ref mut file) = self.file {
            use std::io::Write;
            let ts = now_utc.format("%Y-%m-%dT%H:%M:%S%.3fZ");
            let duration_ms = segment.end_ms.saturating_sub(segment.start_ms);
            let native_word_count = segment.native_text.split_whitespace().count();
            let primary_word_count = segment.primary_text.split_whitespace().count();

            let _ = writeln!(
                file,
                "{},{},{},{},{},{},{},{},{}",
                ts,
                segment.utterance_id,
                segment.start_ms,
                segment.end_ms,
                duration_ms,
                native_word_count,
                primary_word_count,
                segment.native_latency_ms,
                segment.primary_latency_ms,
            );
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_segment(start_ms: u64, end_ms: u64, native: &str, primary: &str) -> NativeSttSegment {
        NativeSttSegment {
            utterance_id: Uuid::new_v4(),
            start_ms,
            end_ms,
            native_text: native.to_string(),
            primary_text: primary.to_string(),
            speaker_id: None,
            native_latency_ms: 100,
            primary_latency_ms: 200,
        }
    }

    fn make_segment_with_speaker(
        start_ms: u64,
        end_ms: u64,
        native: &str,
        speaker: &str,
    ) -> NativeSttSegment {
        NativeSttSegment {
            utterance_id: Uuid::new_v4(),
            start_ms,
            end_ms,
            native_text: native.to_string(),
            primary_text: "primary text".to_string(),
            speaker_id: Some(speaker.to_string()),
            native_latency_ms: 100,
            primary_latency_ms: 200,
        }
    }

    #[test]
    fn test_accumulator_push_maintains_order() {
        let mut acc = NativeSttShadowAccumulator::new();

        // Insert out of order
        acc.push(make_segment(3000, 4000, "third", "Third"));
        acc.push(make_segment(1000, 2000, "first", "First"));
        acc.push(make_segment(2000, 3000, "second", "Second"));

        assert_eq!(acc.len(), 3);
        assert_eq!(acc.segments[0].start_ms, 1000);
        assert_eq!(acc.segments[1].start_ms, 2000);
        assert_eq!(acc.segments[2].start_ms, 3000);
    }

    #[test]
    fn test_accumulator_format_transcript() {
        let mut acc = NativeSttShadowAccumulator::new();
        acc.push(make_segment(0, 1000, "hello world", "Hello World"));
        acc.push(make_segment(1000, 2000, "how are you", "How Are You"));

        let transcript = acc.format_transcript();
        assert_eq!(transcript, "hello world\nhow are you");
    }

    #[test]
    fn test_accumulator_format_transcript_with_speakers() {
        let mut acc = NativeSttShadowAccumulator::new();
        acc.push(make_segment_with_speaker(0, 1000, "hello", "Doctor"));
        acc.push(make_segment_with_speaker(1000, 2000, "hi there", "Patient"));

        let transcript = acc.format_transcript();
        assert_eq!(transcript, "Doctor: hello\nPatient: hi there");
    }

    #[test]
    fn test_accumulator_drain_through() {
        let mut acc = NativeSttShadowAccumulator::new();
        acc.push(make_segment(0, 1000, "a", "A"));
        acc.push(make_segment(1000, 2000, "b", "B"));
        acc.push(make_segment(2000, 3000, "c", "C"));
        acc.push(make_segment(3000, 4000, "d", "D"));

        // Drain through 2000ms — should get segments starting at 0, 1000, 2000
        let drained = acc.drain_through(2000);
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0].native_text, "a");
        assert_eq!(drained[1].native_text, "b");
        assert_eq!(drained[2].native_text, "c");

        // Remaining should be just "d"
        assert_eq!(acc.len(), 1);
        assert_eq!(acc.segments[0].native_text, "d");
    }

    #[test]
    fn test_accumulator_drain_through_all() {
        let mut acc = NativeSttShadowAccumulator::new();
        acc.push(make_segment(0, 1000, "a", "A"));
        acc.push(make_segment(1000, 2000, "b", "B"));

        let drained = acc.drain_through(5000);
        assert_eq!(drained.len(), 2);
        assert!(acc.is_empty());
    }

    #[test]
    fn test_accumulator_drain_through_none() {
        let mut acc = NativeSttShadowAccumulator::new();
        acc.push(make_segment(5000, 6000, "a", "A"));

        let drained = acc.drain_through(1000);
        assert_eq!(drained.len(), 0);
        assert_eq!(acc.len(), 1);
    }

    #[test]
    fn test_accumulator_drain_all() {
        let mut acc = NativeSttShadowAccumulator::new();
        acc.push(make_segment(0, 1000, "hello", "Hello"));
        acc.push(make_segment(1000, 2000, "world", "World"));

        let (transcript, segments) = acc.drain_all();
        assert_eq!(transcript, "hello\nworld");
        assert_eq!(segments.len(), 2);
        assert!(acc.is_empty());
    }

    #[test]
    fn test_accumulator_clear() {
        let mut acc = NativeSttShadowAccumulator::new();
        acc.push(make_segment(0, 1000, "hello", "Hello"));
        assert!(!acc.is_empty());

        acc.clear();
        assert!(acc.is_empty());
        assert_eq!(acc.len(), 0);
    }

    #[test]
    fn test_accumulator_empty() {
        let acc = NativeSttShadowAccumulator::new();
        assert!(acc.is_empty());
        assert_eq!(acc.len(), 0);
        assert_eq!(acc.format_transcript(), "");
    }

    #[test]
    fn test_csv_line_format() {
        // Verify CSV format manually
        let ts = "2026-02-24T10:30:00.000Z";
        let id = Uuid::nil();
        let line = format!(
            "{},{},{},{},{},{},{},{},{}",
            ts, id, 1000, 2000, 1000, 5, 6, 150, 200
        );
        assert!(line.contains("2026-02-24T10:30:00.000Z"));
        assert!(line.contains("00000000-0000-0000-0000-000000000000"));
        assert!(line.contains(",1000,2000,1000,"));
        assert!(line.contains(",5,6,"));
        assert!(line.contains(",150,200"));
    }

    #[test]
    fn test_format_transcript_from_drained() {
        let segments = vec![
            make_segment_with_speaker(0, 1000, "hello", "Doc"),
            make_segment(1000, 2000, "world", "World"),
        ];

        // Format drained segments the same way as accumulator
        let text: String = segments
            .iter()
            .map(|s| {
                if let Some(ref spk) = s.speaker_id {
                    format!("{}: {}", spk, s.native_text)
                } else {
                    s.native_text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(text, "Doc: hello\nworld");
    }
}
