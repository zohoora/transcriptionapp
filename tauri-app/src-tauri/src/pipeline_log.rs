//! Pipeline replay logging for continuous mode.
//!
//! Writes one JSONL line per pipeline step (detection, clinical check, merge,
//! SOAP, vision, hallucination filter) into each session's archive folder.
//! Contains PHI — stored alongside existing PHI (transcript, SOAP) in the archive.

use chrono::Utc;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::warn;

const LOG_FILENAME: &str = "pipeline_log.jsonl";

/// Appends structured JSONL events to a session's archive folder.
/// Created per continuous-mode run; path updates when a new session_id is assigned.
/// Buffers entries in memory when no path is set (before the session archive folder
/// exists), then flushes them to disk when `set_session()` is called.
pub struct PipelineLogger {
    path: Option<PathBuf>,
    /// Entries buffered while `path` is `None` (pre-split detection calls).
    pending: Vec<String>,
}

/// A single pipeline log entry serialized as one JSONL line.
#[derive(Debug, Serialize)]
struct LogEntry {
    ts: String,
    step: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Step-specific context (word counts, flags, thresholds, parsed results, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<serde_json::Value>,
}

impl PipelineLogger {
    /// Create a new logger with no path (call `set_session` before logging).
    pub fn new() -> Self {
        Self { path: None, pending: Vec::new() }
    }

    /// Set the archive directory for the current session.
    /// Flushes any buffered entries that were logged before the path was known.
    pub fn set_session(&mut self, session_dir: &Path) {
        let path = session_dir.join(LOG_FILENAME);
        // Flush pending entries accumulated before the session directory existed
        if !self.pending.is_empty() {
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                for line in self.pending.drain(..) {
                    if let Err(e) = writeln!(f, "{}", line) {
                        warn!("Pipeline log flush failed: {}", e);
                    }
                }
            } else {
                warn!("Pipeline log: could not open {} to flush {} pending entries",
                    path.display(), self.pending.len());
            }
        }
        self.path = Some(path);
    }

    /// Clear the session path (between encounters).
    /// Discards any unflushed pending entries.
    pub fn clear_session(&mut self) {
        self.path = None;
        self.pending.clear();
    }

    /// Append a log entry. Buffers in memory if no path is set yet.
    /// Never blocks the pipeline on I/O errors.
    fn append(&mut self, entry: LogEntry) {
        let line = match serde_json::to_string(&entry) {
            Ok(l) => l,
            Err(e) => {
                warn!("Pipeline log serialization failed: {}", e);
                return;
            }
        };
        match &self.path {
            Some(path) => {
                if let Err(e) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .and_then(|mut f| writeln!(f, "{}", line))
                {
                    warn!("Pipeline log write failed: {}", e);
                }
            }
            None => {
                // Buffer for later flush when set_session() is called
                self.pending.push(line);
            }
        }
    }

    /// Log an LLM call (detection, clinical check, merge, SOAP, vision).
    pub fn log_llm_call(
        &mut self,
        step: &str,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
        response_raw: Option<&str>,
        latency_ms: u64,
        success: bool,
        error: Option<&str>,
        context: serde_json::Value,
    ) {
        self.append(LogEntry {
            ts: Utc::now().to_rfc3339(),
            step: step.to_string(),
            model: Some(model.to_string()),
            prompt_system: Some(system_prompt.to_string()),
            prompt_user: Some(user_prompt.to_string()),
            response_raw: response_raw.map(|s| s.to_string()),
            latency_ms: Some(latency_ms),
            success: Some(success),
            error: error.map(|s| s.to_string()),
            context: Some(context),
        });
    }

    /// Convenience wrappers for specific LLM call steps.
    pub fn log_detection(&mut self, model: &str, system_prompt: &str, user_prompt: &str,
        response_raw: Option<&str>, latency_ms: u64, success: bool, error: Option<&str>,
        context: serde_json::Value) {
        self.log_llm_call("encounter_detection", model, system_prompt, user_prompt, response_raw, latency_ms, success, error, context);
    }
    pub fn log_clinical_check(&mut self, model: &str, system_prompt: &str, user_prompt: &str,
        response_raw: Option<&str>, latency_ms: u64, success: bool, error: Option<&str>,
        context: serde_json::Value) {
        self.log_llm_call("clinical_content_check", model, system_prompt, user_prompt, response_raw, latency_ms, success, error, context);
    }
    pub fn log_merge_check(&mut self, model: &str, system_prompt: &str, user_prompt: &str,
        response_raw: Option<&str>, latency_ms: u64, success: bool, error: Option<&str>,
        context: serde_json::Value) {
        self.log_llm_call("encounter_merge", model, system_prompt, user_prompt, response_raw, latency_ms, success, error, context);
    }
    pub fn log_soap(&mut self, model: &str, system_prompt: &str, user_prompt: &str,
        response_raw: Option<&str>, latency_ms: u64, success: bool, error: Option<&str>,
        context: serde_json::Value) {
        self.log_llm_call("soap_generation", model, system_prompt, user_prompt, response_raw, latency_ms, success, error, context);
    }
    pub fn log_vision(&mut self, model: &str, system_prompt: &str, user_prompt: &str,
        response_raw: Option<&str>, latency_ms: u64, success: bool, error: Option<&str>,
        context: serde_json::Value) {
        self.log_llm_call("vision_extraction", model, system_prompt, user_prompt, response_raw, latency_ms, success, error, context);
    }

    /// Log a pipeline event (hallucination filter, confidence gate, split trigger).
    pub fn log_event(&mut self, step: &str, context: serde_json::Value) {
        self.append(LogEntry {
            ts: Utc::now().to_rfc3339(),
            step: step.to_string(),
            model: None,
            prompt_system: None,
            prompt_user: None,
            response_raw: None,
            latency_ms: None,
            success: None,
            error: None,
            context: Some(context),
        });
    }

    /// Convenience wrappers for specific event steps.
    pub fn log_hallucination_filter(&mut self, context: serde_json::Value) {
        self.log_event("hallucination_filter", context);
    }
    pub fn log_confidence_gate(&mut self, context: serde_json::Value) {
        self.log_event("confidence_gate", context);
    }
    pub fn log_split_trigger(&mut self, context: serde_json::Value) {
        self.log_event("split_trigger", context);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_new_logger_has_no_path() {
        let logger = PipelineLogger::new();
        assert!(logger.path.is_none());
    }

    #[test]
    fn test_set_and_clear_session() {
        let mut logger = PipelineLogger::new();
        let dir = PathBuf::from("/tmp/test-session");
        logger.set_session(&dir);
        assert_eq!(logger.path, Some(dir.join(LOG_FILENAME)));
        logger.clear_session();
        assert!(logger.path.is_none());
    }

    #[test]
    fn test_append_no_path_buffers_pending() {
        let mut logger = PipelineLogger::new();
        logger.log_hallucination_filter(serde_json::json!({"test": true}));
        assert_eq!(logger.pending.len(), 1, "Entry should be buffered when no path set");
    }

    #[test]
    fn test_log_detection_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(dir.path());

        logger.log_detection(
            "fast-model",
            "system prompt",
            "user prompt",
            Some(r#"{"complete": false, "confidence": 0.9}"#),
            1234,
            true,
            None,
            serde_json::json!({
                "word_count": 500,
                "sensor_present": true,
                "consecutive_no_split": 0
            }),
        );

        let log_path = dir.path().join(LOG_FILENAME);
        assert!(log_path.exists());
        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 1);

        let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry["step"], "encounter_detection");
        assert_eq!(entry["model"], "fast-model");
        assert_eq!(entry["prompt_system"], "system prompt");
        assert_eq!(entry["latency_ms"], 1234);
        assert_eq!(entry["success"], true);
        assert_eq!(entry["context"]["word_count"], 500);
    }

    #[test]
    fn test_multiple_entries_append() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(dir.path());

        logger.log_hallucination_filter(serde_json::json!({"original_words": 100}));
        logger.log_detection(
            "fast-model", "sys", "usr", Some("resp"), 100, true, None,
            serde_json::json!({}),
        );
        logger.log_clinical_check(
            "fast-model", "sys", "usr", Some("resp"), 200, true, None,
            serde_json::json!({}),
        );

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 3);

        let e0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let e1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        let e2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(e0["step"], "hallucination_filter");
        assert_eq!(e1["step"], "encounter_detection");
        assert_eq!(e2["step"], "clinical_content_check");
    }

    #[test]
    fn test_log_error_case() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(dir.path());

        logger.log_detection(
            "fast-model", "sys", "usr", None, 5000, false,
            Some("Timeout after 60s"),
            serde_json::json!({"word_count": 2000}),
        );

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let entry: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry["success"], false);
        assert_eq!(entry["error"], "Timeout after 60s");
        assert!(entry["response_raw"].is_null());
    }

    #[test]
    fn test_log_soap_generation() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(dir.path());

        logger.log_soap(
            "soap-model-fast", "sys", "usr", Some("SOAP content"), 8500, true, None,
            serde_json::json!({
                "word_count": 2000,
                "detail_level": 7,
                "format": "problem_based",
                "has_notes": true,
            }),
        );

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let entry: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry["step"], "soap_generation");
        assert_eq!(entry["context"]["detail_level"], 7);
    }

    #[test]
    fn test_log_vision_extraction() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(dir.path());

        logger.log_vision(
            "vision-model", "sys", "extract name", Some("John Smith"), 2800, true, None,
            serde_json::json!({
                "parsed_name": "John Smith",
                "screenshot_blank": false,
                "vote_count": 3,
                "is_stale": false,
            }),
        );

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let entry: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry["step"], "vision_extraction");
        assert_eq!(entry["context"]["parsed_name"], "John Smith");
    }

    #[test]
    fn test_log_merge_check() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(dir.path());

        logger.log_merge_check(
            "fast-model", "sys", "usr", Some(r#"{"same_encounter": false}"#),
            2100, true, None,
            serde_json::json!({
                "prev_session_id": "abc",
                "curr_session_id": "def",
                "patient_name": "John Smith",
            }),
        );

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let entry: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry["step"], "encounter_merge");
        assert_eq!(entry["context"]["prev_session_id"], "abc");
    }

    #[test]
    fn test_log_confidence_gate() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(dir.path());

        logger.log_confidence_gate(serde_json::json!({
            "confidence": 0.72,
            "threshold": 0.85,
            "buffer_age_mins": 12,
            "rejected": true,
        }));

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let entry: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry["step"], "confidence_gate");
        assert_eq!(entry["context"]["rejected"], true);
    }

    #[test]
    fn test_log_split_trigger() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(dir.path());

        logger.log_split_trigger(serde_json::json!({
            "trigger": "manual",
            "word_count": 1500,
        }));

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let entry: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry["step"], "split_trigger");
        assert_eq!(entry["context"]["trigger"], "manual");
    }

    #[test]
    fn test_pending_entries_flushed_on_set_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();

        // Log entries before session dir exists (simulates detection during encounter)
        logger.log_detection(
            "fast-model", "sys", "usr", Some("resp1"), 100, true, None,
            serde_json::json!({"word_count": 500}),
        );
        logger.log_detection(
            "fast-model", "sys", "usr", Some("resp2"), 200, true, None,
            serde_json::json!({"word_count": 800}),
        );
        logger.log_hallucination_filter(serde_json::json!({"original_words": 1000}));

        // Verify entries are buffered, not on disk
        assert_eq!(logger.pending.len(), 3);
        let log_path = dir.path().join(LOG_FILENAME);
        assert!(!log_path.exists());

        // set_session should flush all pending entries
        logger.set_session(dir.path());
        assert!(logger.pending.is_empty(), "Pending should be empty after flush");

        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 3);

        // Verify order preserved
        let e0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let e1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        let e2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(e0["step"], "encounter_detection");
        assert_eq!(e0["context"]["word_count"], 500);
        assert_eq!(e1["step"], "encounter_detection");
        assert_eq!(e1["context"]["word_count"], 800);
        assert_eq!(e2["step"], "hallucination_filter");
    }

    #[test]
    fn test_clear_session_discards_pending() {
        let mut logger = PipelineLogger::new();
        logger.log_hallucination_filter(serde_json::json!({"test": true}));
        assert_eq!(logger.pending.len(), 1);
        logger.clear_session();
        assert!(logger.pending.is_empty());
    }
}
