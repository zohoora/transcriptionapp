//! Screenshot capture task for continuous mode.
//!
//! Periodically captures the screen and buffers JPEGs to the session archive.
//! Makes no LLM calls — patient name + DOB extraction happens in the
//! end-of-encounter SOAP call (`encounter_pipeline::generate_and_archive_soap`),
//! which consumes these JPEGs via
//! `screenshot_dedup::load_deduped_screenshots_for_session`.

use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::config::Config;

/// All inputs needed by the screenshot task, gathered at spawn time.
///
/// `screenshot_interval` is the cadence in seconds (default 30, clamped
/// [10, 60]). The remaining fields plumb through the capture buffer + the
/// transcript-word-count gate (skip capture when the buffer is empty — no
/// speech, no need to spend CPU on a screenshot).
pub struct ScreenshotTaskConfig {
    pub stop_flag: Arc<AtomicBool>,
    pub debug_storage: bool,
    pub screenshot_interval: u64,
    /// Buffer for saving screenshots to session archive after encounter split.
    /// `flush_screenshots_to_session()` (below) drains this at split time and
    /// writes the JPEGs to `session_dir/screenshots/NNN_<ts>.jpg`.
    pub screenshot_buffer: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    /// Transcript buffer used to gate captures: skip when no words are present
    /// (no speech = empty room = no need to burn cycles on a screenshot).
    pub transcript_buffer: Arc<std::sync::Mutex<crate::transcript_buffer::TranscriptBuffer>>,
}

/// Runs the screenshot capture loop.
/// Called via `tokio::spawn(run_screenshot_task(config))`.
///
/// Capture cadence:
///   1. Sleep `screenshot_interval` seconds.
///   2. Check stop_flag → exit if set.
///   3. Gate on `transcript_buffer.word_count() == 0` → skip iteration.
///   4. Capture (blocking CoreGraphics call via `spawn_blocking`).
///   5. Heuristic blank-frame check (left-2/3 grid >80% dark = likely no
///      screen recording permission) → skip buffering, warn loudly.
///   6. Push JPEG bytes into `screenshot_buffer`. (And optionally a debug copy
///      to `~/.transcriptionapp/debug/continuous-screenshots/` when
///      `debug_storage` is on.)
///
/// All vision LLM-call branches (parsing, voting, stale-vote suppression,
/// `PatientNameUpdated` event emission, DOB invalidation) were removed —
/// identity is now extracted by the end-of-encounter SOAP call.
pub async fn run_screenshot_task(cfg: ScreenshotTaskConfig) {
    info!(
        event = "screenshot_task_started",
        interval_secs = cfg.screenshot_interval,
        "Screenshot capture task started"
    );

    let mut captures: u64 = 0;
    let mut skipped_empty: u64 = 0;
    let mut skipped_blank: u64 = 0;
    let mut errors: u64 = 0;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(cfg.screenshot_interval)).await;

        if cfg.stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // Skip capture when no speech in buffer (no active encounter)
        let buffer_words = cfg.transcript_buffer.lock()
            .map(|b| b.word_count())
            .unwrap_or(0);
        if buffer_words == 0 {
            skipped_empty += 1;
            debug!("Screenshot: no words in buffer, skipping capture");
            continue;
        }

        // Capture screen (blocking CoreGraphics call)
        let capture_result =
            tokio::task::spawn_blocking(|| crate::screenshot::capture_to_base64(1150)).await;

        let capture = match capture_result {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                errors += 1;
                debug!("Screenshot capture failed (may not have permission): {}", e);
                continue;
            }
            Err(e) => {
                errors += 1;
                debug!("Screenshot capture task panicked: {}", e);
                continue;
            }
        };

        if capture.likely_blank {
            skipped_blank += 1;
            warn!(
                event = "screenshot_likely_blank",
                "Screenshot appears blank — screen recording permission likely not granted. \
                Grant permission in System Settings → Privacy & Security → Screen Recording."
            );
            continue;
        }

        let image_base64 = capture.base64;

        // Save debug screenshot if enabled
        if cfg.debug_storage {
            save_debug_screenshot(&image_base64);
        }

        // Buffer screenshot for session archive (decoded from base64 to raw JPEG)
        if let Ok(jpeg_bytes) = base64_decode(&image_base64) {
            let ts = Utc::now().to_rfc3339();
            if let Ok(mut buf) = cfg.screenshot_buffer.lock() {
                buf.push((ts, jpeg_bytes));
                captures += 1;
            }
        }
    }

    info!(
        event = "screenshot_task_stopped",
        captures,
        skipped_empty,
        skipped_blank,
        errors,
        "Screenshot capture task stopped"
    );
}

/// Save a debug screenshot to disk.
fn save_debug_screenshot(image_base64: &str) {
    use base64::Engine;
    if let Ok(config_dir) = Config::config_dir() {
        let debug_dir = config_dir.join("debug").join("continuous-screenshots");
        let _ = std::fs::create_dir_all(&debug_dir);
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let filename = debug_dir.join(format!("{}.jpg", timestamp));
        match base64::engine::general_purpose::STANDARD.decode(image_base64) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&filename, &bytes) {
                    warn!("Failed to save debug screenshot: {}", e);
                } else {
                    debug!("Debug screenshot saved: {:?}", filename);
                }
            }
            Err(e) => {
                warn!(
                    "Failed to decode screenshot base64 for debug save: {}",
                    e
                );
            }
        }
    }
}

/// Decode base64-encoded image data to raw bytes.
fn base64_decode(data: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(data)
}

/// Flush buffered screenshots to a session's archive directory.
/// Called after encounter split when the session_dir is known.
pub fn flush_screenshots_to_session(
    buffer: &Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    session_dir: &std::path::Path,
) {
    let screenshots = match buffer.lock() {
        Ok(mut buf) => std::mem::take(&mut *buf),
        Err(e) => {
            warn!("Screenshot buffer lock poisoned: {e}");
            return;
        }
    };
    if screenshots.is_empty() {
        return;
    }
    let dir = session_dir.join("screenshots");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!("Failed to create screenshots dir: {e}");
        return;
    }
    for (i, (ts, jpeg)) in screenshots.iter().enumerate() {
        // Use index + truncated timestamp for filename (avoids colons in filenames)
        let safe_ts = ts.replace(':', "").replace('+', "").chars().take(15).collect::<String>();
        let filename = format!("{:03}_{}.jpg", i, safe_ts);
        if let Err(e) = std::fs::write(dir.join(&filename), jpeg) {
            warn!("Failed to save screenshot {}: {e}", filename);
        }
    }
    info!(
        event = "screenshot_flush",
        count = screenshots.len(),
        path = %dir.display(),
        "Flushed buffered screenshots to session archive"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use crate::transcript_buffer::TranscriptBuffer;

    /// ScreenshotTaskConfig should be constructable with just the fields the
    /// post-consolidation task needs (no name_tracker, no thresholds, no
    /// LLM client, no app_handle). Type-level smoke test.
    #[test]
    fn screenshot_task_config_minimal_fields() {
        let cfg = ScreenshotTaskConfig {
            stop_flag: Arc::new(AtomicBool::new(false)),
            debug_storage: false,
            screenshot_interval: 30,
            screenshot_buffer: Arc::new(Mutex::new(Vec::new())),
            transcript_buffer: Arc::new(std::sync::Mutex::new(TranscriptBuffer::new())),
        };
        assert_eq!(cfg.screenshot_interval, 30);
        assert!(cfg.screenshot_buffer.lock().unwrap().is_empty());
    }

    /// `flush_screenshots_to_session` is a no-op when the buffer is empty.
    #[test]
    fn flush_screenshots_empty_buffer_noop() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let tmp = tempfile::tempdir().unwrap();
        flush_screenshots_to_session(&buffer, tmp.path());
        // No screenshots dir created
        assert!(!tmp.path().join("screenshots").exists());
    }

    /// `flush_screenshots_to_session` writes JPEGs with chronological filenames.
    #[test]
    fn flush_screenshots_writes_chronological_filenames() {
        let buffer = Arc::new(Mutex::new(vec![
            ("2026-05-13T10:00:00+00:00".to_string(), vec![0xFF, 0xD8, 0xFF, 0xE0]),
            ("2026-05-13T10:00:30+00:00".to_string(), vec![0xFF, 0xD8, 0xFF, 0xE0]),
        ]));
        let tmp = tempfile::tempdir().unwrap();
        flush_screenshots_to_session(&buffer, tmp.path());
        let ss_dir = tmp.path().join("screenshots");
        assert!(ss_dir.exists());
        let mut entries: Vec<String> = std::fs::read_dir(&ss_dir).unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        entries.sort();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].starts_with("000_"), "first frame should be 000_*: {}", entries[0]);
        assert!(entries[1].starts_with("001_"), "second frame should be 001_*: {}", entries[1]);
        // Buffer drained after flush
        assert!(buffer.lock().unwrap().is_empty());
    }
}
