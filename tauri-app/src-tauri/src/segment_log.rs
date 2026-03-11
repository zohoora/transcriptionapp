//! Segment timeline logging for continuous mode.
//!
//! Writes one JSONL line per transcript segment into each session's archive folder.
//! Contains PHI — stored alongside existing PHI (transcript, SOAP) in the archive.

use chrono::Utc;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::warn;

const LOG_FILENAME: &str = "segments.jsonl";

/// Appends one JSONL line per transcript segment to a session's archive folder.
/// Created per continuous-mode run; path updates when a new session_id is assigned.
/// Buffers entries in memory when no path is set (before the session archive folder
/// exists), then flushes them to disk when `set_session()` is called.
///
/// Holds an open file handle to avoid per-segment open/close overhead (~4800/day).
pub struct SegmentLogger {
    path: Option<PathBuf>,
    /// Open file handle (kept open between `set_session` and `clear_session`).
    file: Option<File>,
    /// Entries buffered while `path` is `None` (segments received before session dir exists).
    pending: Vec<String>,
}

/// A single segment entry serialized as one JSONL line.
#[derive(Debug, Serialize)]
struct SegmentEntry {
    /// RFC3339 wall-clock when segment was received.
    ts: String,
    /// Monotonic sequence number.
    index: u64,
    /// Audio clock start (milliseconds).
    start_ms: u64,
    /// Audio clock end (milliseconds).
    end_ms: u64,
    /// Transcribed text.
    text: String,
    /// Speaker identifier, if diarization assigned one.
    #[serde(skip_serializing_if = "Option::is_none")]
    speaker_id: Option<String>,
    /// Confidence of speaker assignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    speaker_confidence: Option<f32>,
    /// Number of words in this segment.
    word_count: usize,
    /// Cumulative buffer word count after adding this segment.
    buffer_word_count: usize,
}

impl SegmentLogger {
    /// Create a new logger with no path (call `set_session` before logging).
    pub fn new() -> Self {
        Self {
            path: None,
            file: None,
            pending: Vec::new(),
        }
    }

    /// Set the archive directory for the current session.
    /// Opens a file handle and flushes any buffered entries.
    pub fn set_session(&mut self, session_dir: &Path) {
        let path = session_dir.join(LOG_FILENAME);
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(mut f) => {
                // Flush pending entries accumulated before the session directory existed
                for line in self.pending.drain(..) {
                    if let Err(e) = writeln!(f, "{}", line) {
                        warn!("Segment log flush failed: {}", e);
                    }
                }
                self.file = Some(f);
            }
            Err(e) => {
                warn!(
                    "Segment log: could not open {} to flush {} pending entries: {}",
                    path.display(),
                    self.pending.len(),
                    e,
                );
            }
        }
        self.path = Some(path);
    }

    /// Clear the session path (between encounters).
    /// Closes the file handle and discards any unflushed pending entries.
    pub fn clear_session(&mut self) {
        self.file = None; // Drop closes the file
        self.path = None;
        self.pending.clear();
    }

    /// Log a transcript segment. Buffers in memory if no path is set yet.
    /// Never blocks the pipeline on I/O errors.
    pub fn log_segment(
        &mut self,
        index: u64,
        start_ms: u64,
        end_ms: u64,
        text: &str,
        speaker_id: Option<&str>,
        speaker_confidence: Option<f32>,
        word_count: usize,
        buffer_word_count: usize,
    ) {
        let entry = SegmentEntry {
            ts: Utc::now().to_rfc3339(),
            index,
            start_ms,
            end_ms,
            text: text.to_string(),
            speaker_id: speaker_id.map(|s| s.to_string()),
            speaker_confidence,
            word_count,
            buffer_word_count,
        };
        self.append(entry);
    }

    /// Append a segment entry. Writes to open file handle or buffers if no session set.
    fn append(&mut self, entry: SegmentEntry) {
        let line = match serde_json::to_string(&entry) {
            Ok(l) => l,
            Err(e) => {
                warn!("Segment log serialization failed: {}", e);
                return;
            }
        };
        if let Some(ref mut f) = self.file {
            if let Err(e) = writeln!(f, "{}", line) {
                warn!("Segment log write failed: {}", e);
            }
        } else {
            // Buffer for later flush when set_session() is called
            self.pending.push(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_new_logger_has_no_path() {
        let logger = SegmentLogger::new();
        assert!(logger.path.is_none());
        assert!(logger.file.is_none());
        assert!(logger.pending.is_empty());
    }

    #[test]
    fn test_set_and_clear_session() {
        let mut logger = SegmentLogger::new();
        let dir = tempfile::tempdir().unwrap();
        logger.set_session(dir.path());
        assert_eq!(logger.path, Some(dir.path().join(LOG_FILENAME)));
        assert!(logger.file.is_some());
        logger.clear_session();
        assert!(logger.path.is_none());
        assert!(logger.file.is_none());
    }

    #[test]
    fn test_log_segment_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = SegmentLogger::new();
        logger.set_session(dir.path());

        logger.log_segment(
            0,
            1000,
            2500,
            "The patient reports chest pain",
            Some("speaker_1"),
            Some(0.92),
            5,
            5,
        );

        // Drop file handle so writes are flushed
        logger.clear_session();

        let log_path = dir.path().join(LOG_FILENAME);
        assert!(log_path.exists());
        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 1);

        let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry["index"], 0);
        assert_eq!(entry["start_ms"], 1000);
        assert_eq!(entry["end_ms"], 2500);
        assert_eq!(entry["text"], "The patient reports chest pain");
        assert_eq!(entry["speaker_id"], "speaker_1");
        assert_eq!(entry["word_count"], 5);
        assert_eq!(entry["buffer_word_count"], 5);
        // speaker_confidence is a float — check it's approximately correct
        assert!((entry["speaker_confidence"].as_f64().unwrap() - 0.92).abs() < 0.01);
        // ts should be present and non-empty
        assert!(entry["ts"].as_str().unwrap().len() > 0);
    }

    #[test]
    fn test_log_segment_without_speaker() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = SegmentLogger::new();
        logger.set_session(dir.path());

        logger.log_segment(
            0,
            0,
            1000,
            "Hello",
            None,
            None,
            1,
            1,
        );

        logger.clear_session();

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let entry: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        // speaker_id and speaker_confidence should be absent (skip_serializing_if)
        assert!(entry.get("speaker_id").is_none());
        assert!(entry.get("speaker_confidence").is_none());
    }

    #[test]
    fn test_pending_entries_flushed_on_set_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = SegmentLogger::new();

        // Log segments before session dir exists
        logger.log_segment(0, 0, 1000, "First segment", None, None, 2, 2);
        logger.log_segment(1, 1000, 2000, "Second segment", Some("sp1"), Some(0.8), 2, 4);
        logger.log_segment(2, 2000, 3500, "Third segment", Some("sp2"), Some(0.75), 2, 6);

        // Verify entries are buffered, not on disk
        assert_eq!(logger.pending.len(), 3);
        let log_path = dir.path().join(LOG_FILENAME);
        assert!(!log_path.exists());

        // set_session should flush all pending entries
        logger.set_session(dir.path());
        assert!(logger.pending.is_empty(), "Pending should be empty after flush");

        // Close handle to ensure writes are flushed to disk
        logger.clear_session();

        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 3);

        // Verify order preserved
        let e0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let e1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        let e2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(e0["index"], 0);
        assert_eq!(e0["text"], "First segment");
        assert_eq!(e0["buffer_word_count"], 2);
        assert_eq!(e1["index"], 1);
        assert_eq!(e1["text"], "Second segment");
        assert_eq!(e1["buffer_word_count"], 4);
        assert_eq!(e2["index"], 2);
        assert_eq!(e2["text"], "Third segment");
        assert_eq!(e2["buffer_word_count"], 6);
    }

    #[test]
    fn test_clear_session_discards_pending() {
        let mut logger = SegmentLogger::new();
        logger.log_segment(0, 0, 1000, "Test segment", None, None, 2, 2);
        assert_eq!(logger.pending.len(), 1);
        logger.clear_session();
        assert!(logger.pending.is_empty());
    }

    #[test]
    fn test_multiple_segments_append() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = SegmentLogger::new();
        logger.set_session(dir.path());

        logger.log_segment(0, 0, 1500, "Patient arrives", Some("doctor"), Some(0.95), 2, 2);
        logger.log_segment(1, 1500, 3000, "How are you feeling today", Some("doctor"), Some(0.93), 5, 7);
        logger.log_segment(2, 3000, 5000, "I have been having headaches", Some("patient"), Some(0.88), 6, 13);

        logger.clear_session();

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 3);

        let e0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let e1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        let e2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();

        assert_eq!(e0["index"], 0);
        assert_eq!(e0["text"], "Patient arrives");
        assert_eq!(e0["word_count"], 2);

        assert_eq!(e1["index"], 1);
        assert_eq!(e1["text"], "How are you feeling today");
        assert_eq!(e1["word_count"], 5);

        assert_eq!(e2["index"], 2);
        assert_eq!(e2["text"], "I have been having headaches");
        assert_eq!(e2["speaker_id"], "patient");
        assert_eq!(e2["word_count"], 6);
        assert_eq!(e2["buffer_word_count"], 13);
    }
}
