# Pipeline Replay Logging Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add structured JSONL logging of every LLM call (prompts, responses, latency, context) to each session's archive folder, enabling full pipeline simulation and replay without live patients.

**Architecture:** A new `pipeline_log.rs` module provides a `PipelineLogger` that appends typed JSONL events to `pipeline_log.jsonl` inside each session's archive directory. The logger is created per-encounter in the continuous mode detector loop and passed (via `Arc<Mutex<>>`) to all LLM call sites. Each step wraps its LLM call with `Instant::now()` timing and logs the full prompt, raw response, parsed result, and step-specific context.

**Tech Stack:** Rust (serde_json for serialization, std::fs for append-only writes, std::time::Instant for latency)

---

### Task 1: Create `pipeline_log.rs` module

**Files:**
- Create: `tauri-app/src-tauri/src/pipeline_log.rs`
- Modify: `tauri-app/src-tauri/src/lib.rs:32-78` (add `pub mod pipeline_log;`)

**Step 1: Write the test file with unit tests**

In `pipeline_log.rs`, write the module with tests at the bottom:

```rust
//! Pipeline replay logging for continuous mode.
//!
//! Writes one JSONL line per pipeline step (detection, clinical check, merge,
//! SOAP, vision, hallucination filter) into each session's archive folder.
//! Contains PHI — stored alongside existing PHI (transcript, SOAP) in the archive.

use chrono::Utc;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tracing::warn;

const LOG_FILENAME: &str = "pipeline_log.jsonl";

/// Appends structured JSONL events to a session's archive folder.
/// Created per-encounter; path updates when a new session_id is assigned.
pub struct PipelineLogger {
    path: Option<PathBuf>,
}

/// A single pipeline log entry. Each variant captures step-specific data.
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
        Self { path: None }
    }

    /// Set the archive directory for the current session.
    /// Call this after `save_session()` creates the session folder.
    pub fn set_session(&mut self, session_dir: &PathBuf) {
        self.path = Some(session_dir.join(LOG_FILENAME));
    }

    /// Clear the session path (between encounters).
    pub fn clear_session(&mut self) {
        self.path = None;
    }

    /// Append a log entry. Never blocks the pipeline on I/O errors.
    fn append(&self, entry: LogEntry) {
        let path = match &self.path {
            Some(p) => p,
            None => return, // No session set — silently skip
        };
        let line = match serde_json::to_string(&entry) {
            Ok(l) => l,
            Err(e) => {
                warn!("Pipeline log serialization failed: {}", e);
                return;
            }
        };
        if let Err(e) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| writeln!(f, "{}", line))
        {
            warn!("Pipeline log write failed: {}", e);
        }
    }

    /// Log an encounter detection LLM call.
    pub fn log_detection(
        &self,
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
            step: "encounter_detection".to_string(),
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

    /// Log a clinical content check LLM call.
    pub fn log_clinical_check(
        &self,
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
            step: "clinical_content_check".to_string(),
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

    /// Log an encounter merge check LLM call.
    pub fn log_merge_check(
        &self,
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
            step: "encounter_merge".to_string(),
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

    /// Log a SOAP generation LLM call.
    pub fn log_soap(
        &self,
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
            step: "soap_generation".to_string(),
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

    /// Log a vision/patient name extraction call.
    pub fn log_vision(
        &self,
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
            step: "vision_extraction".to_string(),
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

    /// Log hallucination filter results (not an LLM call).
    pub fn log_hallucination_filter(&self, context: serde_json::Value) {
        self.append(LogEntry {
            ts: Utc::now().to_rfc3339(),
            step: "hallucination_filter".to_string(),
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

    /// Log a confidence gate decision (not an LLM call).
    pub fn log_confidence_gate(&self, context: serde_json::Value) {
        self.append(LogEntry {
            ts: Utc::now().to_rfc3339(),
            step: "confidence_gate".to_string(),
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

    /// Log a force-split or manual/sensor trigger (not an LLM call).
    pub fn log_split_trigger(&self, context: serde_json::Value) {
        self.append(LogEntry {
            ts: Utc::now().to_rfc3339(),
            step: "split_trigger".to_string(),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
    fn test_append_no_path_is_noop() {
        // Should not panic when no path is set
        let logger = PipelineLogger::new();
        logger.log_hallucination_filter(serde_json::json!({"test": true}));
    }

    #[test]
    fn test_log_detection_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let mut logger = PipelineLogger::new();
        logger.set_session(&dir.path().to_path_buf());

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
        logger.set_session(&dir.path().to_path_buf());

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
        logger.set_session(&dir.path().to_path_buf());

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
        logger.set_session(&dir.path().to_path_buf());

        logger.log_soap(
            "soap-model-fast", "sys", "usr", Some("SOAP content here"), 8500, true, None,
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
        logger.set_session(&dir.path().to_path_buf());

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
        logger.set_session(&dir.path().to_path_buf());

        logger.log_merge_check(
            "fast-model", "sys", "usr", Some(r#"{"same_encounter": false}"#),
            2100, true, None,
            serde_json::json!({
                "prev_session_id": "abc",
                "curr_session_id": "def",
                "patient_name": "John Smith",
                "prev_words": 500,
                "curr_words": 450,
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
        logger.set_session(&dir.path().to_path_buf());

        logger.log_confidence_gate(serde_json::json!({
            "confidence": 0.72,
            "threshold": 0.85,
            "buffer_age_mins": 12,
            "word_count": 800,
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
        logger.set_session(&dir.path().to_path_buf());

        logger.log_split_trigger(serde_json::json!({
            "trigger": "manual",
            "word_count": 1500,
        }));

        let content = fs::read_to_string(dir.path().join(LOG_FILENAME)).unwrap();
        let entry: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry["step"], "split_trigger");
        assert_eq!(entry["context"]["trigger"], "manual");
    }
}
```

**Step 2: Run test to verify it compiles and tests pass**

Run: `cd tauri-app/src-tauri && cargo test pipeline_log -- --nocapture`
Expected: 10 tests pass

**Step 3: Register the module in `lib.rs`**

Add after `pub mod presence_sensor;` (line 56):
```rust
pub mod pipeline_log;
```

**Step 4: Run full test suite**

Run: `cd tauri-app/src-tauri && cargo test --lib`
Expected: 577+ tests pass

**Step 5: Commit**

```bash
git add tauri-app/src-tauri/src/pipeline_log.rs tauri-app/src-tauri/src/lib.rs
git commit -m "feat: add pipeline_log.rs module for replay logging"
```

---

### Task 2: Add `tempfile` dev-dependency for tests

**Files:**
- Modify: `tauri-app/src-tauri/Cargo.toml`

**Step 1: Check if tempfile is already a dependency**

Run: `grep tempfile tauri-app/src-tauri/Cargo.toml`

**Step 2: Add tempfile if not present**

In `Cargo.toml` under `[dev-dependencies]`, add:
```toml
tempfile = "3"
```

**Step 3: Verify tests pass with tempfile**

Run: `cd tauri-app/src-tauri && cargo test pipeline_log -- --nocapture`
Expected: All pipeline_log tests pass

**Step 4: Commit**

```bash
git add tauri-app/src-tauri/Cargo.toml
git commit -m "chore: add tempfile dev-dependency for pipeline log tests"
```

---

### Task 3: Wire PipelineLogger into the continuous mode detector loop

**Files:**
- Modify: `tauri-app/src-tauri/src/continuous_mode.rs`

This is the largest task — instrumenting every LLM call site in the detector loop.

**Step 1: Create and share the logger**

Near the top of `run_continuous_mode()`, after LLM client creation (~line 700), add:

```rust
use crate::pipeline_log::PipelineLogger;
use std::time::Instant;

let pipeline_logger = Arc::new(Mutex::new(PipelineLogger::new()));
```

Clone for detector task and screenshot task:
```rust
let logger_for_detector = Arc::clone(&pipeline_logger);
let logger_for_screenshot = Arc::clone(&pipeline_logger);
```

**Step 2: Set logger session path after `save_session()` call**

After `save_session()` succeeds (~line 1387), set the logger path:

```rust
// Set pipeline logger to write to this session's archive folder
if let Ok(archive_dir) = local_archive::get_archive_dir() {
    let now_log = Utc::now();
    let session_dir = archive_dir
        .join(format!("{:04}", now_log.year()))
        .join(format!("{:02}", now_log.month()))
        .join(format!("{:02}", now_log.day()))
        .join(&session_id);
    if let Ok(mut logger) = logger_for_detector.lock() {
        logger.set_session(&session_dir);
    }
}
```

**Step 3: Log manual/sensor force-split triggers**

Before the LLM call (~line 1132), when `manual_triggered || sensor_triggered`:

```rust
if let Ok(logger) = logger_for_detector.lock() {
    logger.log_split_trigger(serde_json::json!({
        "trigger": if manual_triggered { "manual" } else { "sensor" },
        "word_count": word_count,
        "cleaned_word_count": cleaned_word_count,
    }));
}
```

**Step 4: Instrument the encounter detection LLM call**

Wrap the LLM call (~line 1176-1220) with timing and logging. The key change: capture `Instant::now()` before the call, capture the raw response string, then log.

Replace the detection block with timing:

```rust
let detect_start = Instant::now();
let llm_future = client.generate(&detection_model, &system_prompt, &user_prompt, "encounter_detection");
match tokio::time::timeout(tokio::time::Duration::from_secs(60), llm_future).await {
    Ok(Ok(response)) => {
        let latency = detect_start.elapsed().as_millis() as u64;
        match parse_encounter_detection(&response) {
            Ok(result) => {
                info!(
                    "Detection result: complete={}, confidence={:?}, end_segment_index={:?}, word_count={}",
                    result.complete, result.confidence, result.end_segment_index, word_count
                );
                if let Ok(logger) = logger_for_detector.lock() {
                    logger.log_detection(
                        &detection_model,
                        &system_prompt,
                        &user_prompt,
                        Some(&response),
                        latency,
                        true,
                        None,
                        serde_json::json!({
                            "word_count": word_count,
                            "cleaned_word_count": cleaned_word_count,
                            "sensor_present": detection_context.sensor_present,
                            "sensor_departed": detection_context.sensor_departed,
                            "vision_triggered": vision_triggered,
                            "current_patient_name": detection_context.current_patient_name,
                            "new_patient_name": detection_context.new_patient_name,
                            "nothink": detection_nothink,
                            "consecutive_no_split": consecutive_no_split,
                            "parsed_complete": result.complete,
                            "parsed_confidence": result.confidence,
                            "parsed_end_segment_index": result.end_segment_index,
                        }),
                    );
                }
                Some(result)
            }
            Err(e) => {
                let latency = detect_start.elapsed().as_millis() as u64;
                warn!("Failed to parse encounter detection: {}", e);
                if let Ok(logger) = logger_for_detector.lock() {
                    logger.log_detection(
                        &detection_model, &system_prompt, &user_prompt,
                        Some(&response), latency, false, Some(&e),
                        serde_json::json!({"word_count": word_count, "parse_error": true}),
                    );
                }
                // ... existing error handling ...
                None
            }
        }
    }
    Ok(Err(e)) => {
        let latency = detect_start.elapsed().as_millis() as u64;
        warn!("Encounter detection LLM call failed: {}", e);
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_detection(
                &detection_model, &system_prompt, &user_prompt,
                None, latency, false, Some(&e.to_string()),
                serde_json::json!({"word_count": word_count, "llm_error": true}),
            );
        }
        // ... existing error handling ...
        None
    }
    Err(_elapsed) => {
        let latency = detect_start.elapsed().as_millis() as u64;
        warn!("Encounter detection LLM call timed out after 60s");
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_detection(
                &detection_model, &system_prompt, &user_prompt,
                None, latency, false, Some("timeout_60s"),
                serde_json::json!({"word_count": word_count, "timeout": true}),
            );
        }
        // ... existing error handling ...
        None
    }
}
```

**Step 5: Log the confidence gate decision**

After the confidence gate check (~line 1319-1334):

```rust
if confidence < confidence_threshold && !force_split {
    consecutive_no_split += 1;
    info!("Confidence gate rejected: ...");
    if let Ok(logger) = logger_for_detector.lock() {
        logger.log_confidence_gate(serde_json::json!({
            "confidence": confidence,
            "threshold": confidence_threshold,
            "buffer_age_mins": buffer_age_mins,
            "word_count": word_count,
            "consecutive_no_split": consecutive_no_split,
            "rejected": true,
        }));
    }
    // ... existing continue ...
}
```

**Step 6: Log hallucination filter results at the detection call site**

After the hallucination filter runs (~line 1063-1068), log the full report:

```rust
if report.repetitions.len() > 0 || report.phrase_repetitions.len() > 0 {
    info!("Hallucination filter: ...");
    if let Ok(logger) = logger_for_detector.lock() {
        logger.log_hallucination_filter(serde_json::json!({
            "call_site": "detection",
            "original_words": report.original_word_count,
            "cleaned_words": report.cleaned_word_count,
            "single_word_reps": report.repetitions.iter()
                .map(|r| &r.word).collect::<Vec<_>>(),
            "phrase_reps": report.phrase_repetitions.iter()
                .map(|r| &r.phrase).collect::<Vec<_>>(),
        }));
    }
}
```

**Step 7: Log force-split triggers**

At the absolute word cap force-split (~line 1234) and graduated force-split (~line 1261):

```rust
// Absolute word cap
if let Ok(logger) = logger_for_detector.lock() {
    logger.log_split_trigger(serde_json::json!({
        "trigger": "absolute_word_cap",
        "cleaned_word_count": cleaned_word_count,
        "raw_word_count": word_count,
        "cap": ABSOLUTE_WORD_CAP,
    }));
}

// Graduated force-split
if let Ok(logger) = logger_for_detector.lock() {
    logger.log_split_trigger(serde_json::json!({
        "trigger": "graduated_force_split",
        "consecutive_no_split": consecutive_no_split,
        "cleaned_word_count": cleaned_word_count,
        "raw_word_count": word_count,
        "threshold": FORCE_SPLIT_WORD_THRESHOLD,
        "limit": FORCE_SPLIT_CONSECUTIVE_LIMIT,
    }));
}
```

**Step 8: Instrument the clinical content check (~line 1629-1681)**

```rust
let cc_start = Instant::now();
let cc_future = client.generate(&fast_model, &cc_system, &cc_user, "clinical_content_check");
match tokio::time::timeout(tokio::time::Duration::from_secs(30), cc_future).await {
    Ok(Ok(cc_response)) => {
        let cc_latency = cc_start.elapsed().as_millis() as u64;
        match parse_clinical_content_check(&cc_response) {
            Ok(cc_result) => {
                // ... existing logic ...
                if let Ok(logger) = logger_for_detector.lock() {
                    logger.log_clinical_check(
                        &fast_model, &cc_system, &cc_user,
                        Some(&cc_response), cc_latency, true, None,
                        serde_json::json!({
                            "encounter_number": encounter_number,
                            "word_count": encounter_word_count,
                            "is_clinical": cc_result.clinical,
                            "reason": cc_result.reason,
                        }),
                    );
                }
            }
            Err(e) => {
                let cc_latency = cc_start.elapsed().as_millis() as u64;
                warn!("Failed to parse clinical content check: {}", e);
                if let Ok(logger) = logger_for_detector.lock() {
                    logger.log_clinical_check(
                        &fast_model, &cc_system, &cc_user,
                        Some(&cc_response), cc_latency, false, Some(&e),
                        serde_json::json!({"encounter_number": encounter_number, "parse_error": true}),
                    );
                }
            }
        }
    }
    Ok(Err(e)) => {
        let cc_latency = cc_start.elapsed().as_millis() as u64;
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_clinical_check(
                &fast_model, &cc_system, &cc_user,
                None, cc_latency, false, Some(&e.to_string()),
                serde_json::json!({"encounter_number": encounter_number, "llm_error": true}),
            );
        }
    }
    Err(_) => {
        let cc_latency = cc_start.elapsed().as_millis() as u64;
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_clinical_check(
                &fast_model, &cc_system, &cc_user,
                None, cc_latency, false, Some("timeout_30s"),
                serde_json::json!({"encounter_number": encounter_number, "timeout": true}),
            );
        }
    }
}
```

**Step 9: Instrument SOAP generation (~line 1697-1758)**

The SOAP generation in `llm_client.rs` builds the prompt internally. To capture prompts, add a `generate_multi_patient_soap_note_with_prompts()` method OR capture the prompt at the call site by calling the prompt builder directly. The simpler approach: capture timing and response at the call site, and log the transcript + options (the prompt can be reconstructed from these).

```rust
let soap_start = Instant::now();
let soap_future = client.generate_multi_patient_soap_note(...);
match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
    Ok(Ok(soap_result)) => {
        let soap_latency = soap_start.elapsed().as_millis() as u64;
        let soap_content = &soap_result.notes.iter()
            .map(|n| n.content.clone()).collect::<Vec<_>>().join("\n\n---\n\n");
        // ... existing save logic ...
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_soap(
                &soap_model, "", "", // Prompt built internally — logged as empty
                Some(soap_content), soap_latency, true, None,
                serde_json::json!({
                    "encounter_number": encounter_number,
                    "word_count": encounter_word_count,
                    "detail_level": soap_detail_level,
                    "format": soap_format,
                    "has_notes": !notes_text.is_empty(),
                    "response_chars": soap_content.len(),
                }),
            );
        }
    }
    Ok(Err(e)) => {
        let soap_latency = soap_start.elapsed().as_millis() as u64;
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_soap(
                &soap_model, "", "", None, soap_latency, false, Some(&e.to_string()),
                serde_json::json!({"encounter_number": encounter_number, "llm_error": true}),
            );
        }
    }
    Err(_) => {
        let soap_latency = soap_start.elapsed().as_millis() as u64;
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_soap(
                &soap_model, "", "", None, soap_latency, false, Some("timeout_120s"),
                serde_json::json!({"encounter_number": encounter_number, "timeout": true}),
            );
        }
    }
}
```

**Step 10: Instrument encounter merge check (~line 1796-1913)**

```rust
let merge_start = Instant::now();
let merge_future = client.generate(&fast_model, &merge_system, &merge_user, "encounter_merge");
match tokio::time::timeout(tokio::time::Duration::from_secs(60), merge_future).await {
    Ok(Ok(merge_response)) => {
        let merge_latency = merge_start.elapsed().as_millis() as u64;
        match parse_merge_check(&merge_response) {
            Ok(merge_result) => {
                if let Ok(logger) = logger_for_detector.lock() {
                    logger.log_merge_check(
                        &fast_model, &merge_system, &merge_user,
                        Some(&merge_response), merge_latency, true, None,
                        serde_json::json!({
                            "prev_session_id": prev_id,
                            "curr_session_id": session_id,
                            "patient_name": merge_patient_name,
                            "prev_words": prev_words.len(),
                            "curr_words": curr_words.len(),
                            "same_encounter": merge_result.same_encounter,
                            "reason": merge_result.reason,
                        }),
                    );
                }
                // ... existing merge logic ...
            }
            Err(e) => {
                let merge_latency = merge_start.elapsed().as_millis() as u64;
                if let Ok(logger) = logger_for_detector.lock() {
                    logger.log_merge_check(
                        &fast_model, &merge_system, &merge_user,
                        Some(&merge_response), merge_latency, false, Some(&e),
                        serde_json::json!({"parse_error": true}),
                    );
                }
            }
        }
    }
    Ok(Err(e)) => {
        let merge_latency = merge_start.elapsed().as_millis() as u64;
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_merge_check(
                &fast_model, &merge_system, &merge_user,
                None, merge_latency, false, Some(&e.to_string()),
                serde_json::json!({"llm_error": true}),
            );
        }
    }
    Err(_) => {
        let merge_latency = merge_start.elapsed().as_millis() as u64;
        if let Ok(logger) = logger_for_detector.lock() {
            logger.log_merge_check(
                &fast_model, &merge_system, &merge_user,
                None, merge_latency, false, Some("timeout_60s"),
                serde_json::json!({"timeout": true}),
            );
        }
    }
}
```

**Step 11: Instrument vision/screenshot name extraction (~line 2043-2119)**

```rust
let vision_start = Instant::now();
let vision_future = client.generate_vision("vision-model", ...);
match tokio::time::timeout(tokio::time::Duration::from_secs(30), vision_future).await {
    Ok(Ok(response)) => {
        let vision_latency = vision_start.elapsed().as_millis() as u64;
        let parsed_name = parse_patient_name(&response);
        if let Ok(logger) = logger_for_screenshot.lock() {
            let vote_count = name_tracker_for_screenshot.lock()
                .ok().map(|t| t.votes.len()).unwrap_or(0);  // Note: votes is private, use majority_name presence as proxy
            logger.log_vision(
                "vision-model", &system_prompt, &user_text,
                Some(&response), vision_latency, true, None,
                serde_json::json!({
                    "parsed_name": parsed_name,
                    "screenshot_blank": false,
                }),
            );
        }
        // ... existing name processing logic ...
    }
    Ok(Err(e)) => {
        let vision_latency = vision_start.elapsed().as_millis() as u64;
        if let Ok(logger) = logger_for_screenshot.lock() {
            logger.log_vision(
                "vision-model", &system_prompt, &user_text,
                None, vision_latency, false, Some(&e.to_string()),
                serde_json::json!({"llm_error": true}),
            );
        }
    }
    Err(_) => {
        let vision_latency = vision_start.elapsed().as_millis() as u64;
        if let Ok(logger) = logger_for_screenshot.lock() {
            logger.log_vision(
                "vision-model", &system_prompt, &user_text,
                None, vision_latency, false, Some("timeout_30s"),
                serde_json::json!({"timeout": true}),
            );
        }
    }
}
```

**Step 12: Clear logger between encounters**

After the encounter is fully processed (after merge check, before `prev_encounter_*` update at ~line 1918):

```rust
if let Ok(mut logger) = logger_for_detector.lock() {
    logger.clear_session();
}
```

**Step 13: Instrument the flush-on-stop SOAP path (~line 2200-2251)**

Similar to Step 9 but for the buffer flush at shutdown:

```rust
// Set logger for flush session
if let Ok(archive_dir) = local_archive::get_archive_dir() {
    let now_flush = Utc::now();
    let flush_dir = archive_dir
        .join(format!("{:04}", now_flush.year()))
        .join(format!("{:02}", now_flush.month()))
        .join(format!("{:02}", now_flush.day()))
        .join(&session_id);
    if let Ok(mut logger) = pipeline_logger.lock() {
        logger.set_session(&flush_dir);
    }
}

let flush_soap_start = Instant::now();
// ... existing soap_future ...
// Log result similar to Step 9
```

**Step 14: Run tests**

Run: `cd tauri-app/src-tauri && cargo check`
Expected: Compiles with no errors

Run: `cd tauri-app/src-tauri && cargo test --lib`
Expected: 577+ tests pass

**Step 15: Commit**

```bash
git add tauri-app/src-tauri/src/continuous_mode.rs
git commit -m "feat: instrument all LLM calls with pipeline replay logging"
```

---

### Task 4: Expose SOAP prompt for logging

**Files:**
- Modify: `tauri-app/src-tauri/src/llm_client.rs`

The SOAP generation builds prompts internally in `generate_multi_patient_soap_note()`. To log full prompts, add a method that returns the built prompts alongside the result.

**Step 1: Add `build_soap_prompts()` public method**

Extract the prompt-building portion of `generate_multi_patient_soap_note()` into a separate public method that returns `(system_prompt, user_content)`:

```rust
/// Build SOAP generation prompts without sending them.
/// Used by pipeline logging to capture the exact prompts sent.
pub fn build_soap_prompts(
    &self,
    transcript: &str,
    audio_events: Option<&[crate::transcription::AudioEvent]>,
    options: Option<&SoapOptions>,
    speaker_context: Option<&SpeakerContext>,
) -> (String, String) {
    let prepared_transcript = Self::prepare_transcript(transcript, audio_events, speaker_context);
    let (system_prompt, user_content) = Self::build_multi_patient_soap_prompt(
        &prepared_transcript,
        options,
        speaker_context,
    );
    (system_prompt, user_content)
}
```

**Step 2: Use it in continuous_mode.rs SOAP logging**

Before the SOAP call, build prompts for logging:
```rust
let (soap_sys, soap_usr) = client.build_soap_prompts(
    &filtered_encounter_text, None, Some(&soap_opts), None,
);
```

Then pass `&soap_sys` and `&soap_usr` to `logger.log_soap()` instead of empty strings.

**Step 3: Run tests**

Run: `cd tauri-app/src-tauri && cargo test --lib`
Expected: All pass

**Step 4: Commit**

```bash
git add tauri-app/src-tauri/src/llm_client.rs tauri-app/src-tauri/src/continuous_mode.rs
git commit -m "feat: expose SOAP prompt builder for pipeline replay logging"
```

---

### Task 5: Log hallucination filter at all call sites

**Files:**
- Modify: `tauri-app/src-tauri/src/continuous_mode.rs`

Currently hallucination filtering is logged only at the detection site (~line 1063). Add logging at the SOAP prep (~line 1689), merge excerpt prep (~lines 1784-1785), and flush SOAP (~line 2180) sites.

**Step 1: Add logging at SOAP hallucination filter**

After `let (filtered_encounter_text, _) = strip_hallucinations(&encounter_text, 5);` (~line 1689):

```rust
let (filtered_encounter_text, soap_filter_report) = strip_hallucinations(&encounter_text, 5);
if soap_filter_report.repetitions.len() > 0 || soap_filter_report.phrase_repetitions.len() > 0 {
    if let Ok(logger) = logger_for_detector.lock() {
        logger.log_hallucination_filter(serde_json::json!({
            "call_site": "soap_prep",
            "original_words": soap_filter_report.original_word_count,
            "cleaned_words": soap_filter_report.cleaned_word_count,
            "single_word_reps": soap_filter_report.repetitions.iter()
                .map(|r| &r.word).collect::<Vec<_>>(),
            "phrase_reps": soap_filter_report.phrase_repetitions.iter()
                .map(|r| &r.phrase).collect::<Vec<_>>(),
        }));
    }
}
```

**Step 2: Add logging at merge hallucination filters**

After the two `strip_hallucinations` calls for merge (~lines 1784-1785):

```rust
let (filtered_prev_tail, prev_filter_report) = strip_hallucinations(&prev_tail, 5);
let (filtered_curr_head, curr_filter_report) = strip_hallucinations(&curr_head, 5);
if prev_filter_report.repetitions.len() > 0 || prev_filter_report.phrase_repetitions.len() > 0
    || curr_filter_report.repetitions.len() > 0 || curr_filter_report.phrase_repetitions.len() > 0
{
    if let Ok(logger) = logger_for_detector.lock() {
        logger.log_hallucination_filter(serde_json::json!({
            "call_site": "merge_prep",
            "prev_original_words": prev_filter_report.original_word_count,
            "prev_cleaned_words": prev_filter_report.cleaned_word_count,
            "curr_original_words": curr_filter_report.original_word_count,
            "curr_cleaned_words": curr_filter_report.cleaned_word_count,
        }));
    }
}
```

**Step 3: Add logging at flush hallucination filter**

After `let (filtered_text, _) = strip_hallucinations(&text, 5);` (~line 2180):

```rust
let (filtered_text, flush_filter_report) = strip_hallucinations(&text, 5);
if flush_filter_report.repetitions.len() > 0 || flush_filter_report.phrase_repetitions.len() > 0 {
    if let Ok(logger) = pipeline_logger.lock() {
        logger.log_hallucination_filter(serde_json::json!({
            "call_site": "flush_soap_prep",
            "original_words": flush_filter_report.original_word_count,
            "cleaned_words": flush_filter_report.cleaned_word_count,
        }));
    }
}
```

**Step 4: Run tests**

Run: `cd tauri-app/src-tauri && cargo test --lib`
Expected: All pass

**Step 5: Commit**

```bash
git add tauri-app/src-tauri/src/continuous_mode.rs
git commit -m "feat: log hallucination filter at all call sites for replay completeness"
```

---

### Task 6: Final verification

**Step 1: Full Rust test suite**

Run: `cd tauri-app/src-tauri && cargo test --lib`
Expected: 580+ tests pass (new pipeline_log tests + existing)

**Step 2: TypeScript check (no frontend changes)**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: No errors

**Step 3: Build the app**

Run: `cd tauri-app && pnpm tauri build --debug`
Expected: Successful build

**Step 4: Manual verification**

1. Start continuous mode
2. Speak a short test encounter, wait for detection
3. Check `~/.transcriptionapp/archive/YYYY/MM/DD/<session_id>/pipeline_log.jsonl`
4. Verify it contains `encounter_detection`, `clinical_content_check`, `soap_generation` entries
5. Verify each entry has `prompt_system`, `prompt_user`, `response_raw`, `latency_ms`, `context`

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat: complete pipeline replay logging for all continuous mode LLM calls"
```

---

## Summary of All Files Modified

| File | Change |
|------|--------|
| `tauri-app/src-tauri/src/pipeline_log.rs` (new) | PipelineLogger struct, JSONL writer, typed log methods, 10 unit tests |
| `tauri-app/src-tauri/src/lib.rs` | Add `pub mod pipeline_log;` |
| `tauri-app/src-tauri/Cargo.toml` | Add `tempfile = "3"` dev-dependency |
| `tauri-app/src-tauri/src/continuous_mode.rs` | Create logger, instrument 7 LLM call sites + 3 hallucination filter sites + confidence gate + force-split triggers |
| `tauri-app/src-tauri/src/llm_client.rs` | Add `build_soap_prompts()` public method |

## What This Enables

With `pipeline_log.jsonl` in each session's archive:

```bash
# Replay a detection prompt with a different model
cat ~/.transcriptionapp/archive/2026/03/04/<id>/pipeline_log.jsonl \
  | jq 'select(.step == "encounter_detection")'

# Compare latencies across encounters
cat ~/.transcriptionapp/archive/2026/03/04/*/pipeline_log.jsonl \
  | jq 'select(.step == "soap_generation") | {session: .context.encounter_number, latency: .latency_ms}'

# Simulate detection with modified prompt
python3 simulate.py --session <id> --step encounter_detection --modify-prompt "add sensor context"
```
