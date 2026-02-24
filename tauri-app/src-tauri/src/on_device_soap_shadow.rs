//! On-Device SOAP Shadow Logging
//!
//! Records comparison metrics between primary LLM Router SOAP and on-device
//! Apple Foundation Models SOAP generation. CSV logs are PHI-safe (word counts
//! and latencies only, no SOAP text).
//!
//! CSV logs written to `~/.transcriptionapp/shadow_soap/YYYY-MM-DD.csv` with daily rotation.

use chrono::Utc;
use std::path::PathBuf;
use tracing::{info, warn};

// ============================================================================
// Types
// ============================================================================

/// Comparison metrics between primary and on-device SOAP generation.
/// PHI-safe: contains only counts and timings, no SOAP text.
pub struct SoapComparisonMetrics {
    pub session_id: String,
    pub primary_word_count: usize,
    pub ondevice_word_count: usize,
    pub primary_latency_ms: u64,
    pub ondevice_latency_ms: u64,
    pub primary_section_count: usize,
    pub ondevice_section_count: usize,
}

// ============================================================================
// CSV Logger
// ============================================================================

/// CSV logger for on-device SOAP shadow comparison — daily rotation to
/// `~/.transcriptionapp/shadow_soap/`.
pub struct OnDeviceSoapCsvLogger {
    log_dir: PathBuf,
    current_date: String,
    file: Option<std::fs::File>,
}

impl OnDeviceSoapCsvLogger {
    /// Create a new logger. Creates the log directory if needed.
    pub fn new() -> Result<Self, String> {
        let log_dir = dirs::home_dir()
            .ok_or("No home directory")?
            .join(".transcriptionapp")
            .join("shadow_soap");

        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create shadow_soap log dir: {}", e))?;

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut logger = OnDeviceSoapCsvLogger {
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
            .map_err(|e| format!("Failed to open shadow SOAP CSV log: {}", e))?;

        if write_header {
            use std::io::Write;
            let mut f = &file;
            let _ = writeln!(
                f,
                "timestamp_utc,session_id,primary_word_count,ondevice_word_count,primary_latency_ms,ondevice_latency_ms,primary_section_count,ondevice_section_count"
            );
        }

        self.file = Some(file);
        info!("Shadow SOAP CSV log opened: {}.csv", self.current_date);
        Ok(())
    }

    /// Write comparison metrics to CSV.
    pub fn log(&mut self, metrics: &SoapComparisonMetrics) {
        let now_utc = Utc::now();

        // Check for midnight rotation
        let today = now_utc.format("%Y-%m-%d").to_string();
        if today != self.current_date {
            self.current_date = today;
            if let Err(e) = self.open_file() {
                warn!("Failed to rotate shadow SOAP CSV log: {}", e);
                return;
            }
            info!("Shadow SOAP CSV log rotated to {}.csv", self.current_date);
        }

        if let Some(ref mut file) = self.file {
            use std::io::Write;
            let ts = now_utc.format("%Y-%m-%dT%H:%M:%S%.3fZ");
            let _ = writeln!(
                file,
                "{},{},{},{},{},{},{},{}",
                ts,
                metrics.session_id,
                metrics.primary_word_count,
                metrics.ondevice_word_count,
                metrics.primary_latency_ms,
                metrics.ondevice_latency_ms,
                metrics.primary_section_count,
                metrics.ondevice_section_count,
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

    #[test]
    fn test_csv_line_format() {
        let ts = "2026-02-24T10:30:00.000Z";
        let line = format!(
            "{},{},{},{},{},{},{},{}",
            ts, "session-123", 450, 380, 2500, 3200, 4, 4
        );

        assert!(line.contains("2026-02-24T10:30:00.000Z"));
        assert!(line.contains("session-123"));
        assert!(line.contains(",450,380,"));
        assert!(line.contains(",2500,3200,"));
        assert!(line.contains(",4,4"));
    }

    #[test]
    fn test_soap_comparison_metrics() {
        let metrics = SoapComparisonMetrics {
            session_id: "test-session".to_string(),
            primary_word_count: 200,
            ondevice_word_count: 180,
            primary_latency_ms: 1500,
            ondevice_latency_ms: 3000,
            primary_section_count: 4,
            ondevice_section_count: 4,
        };

        assert_eq!(metrics.session_id, "test-session");
        assert_eq!(metrics.primary_word_count, 200);
        assert_eq!(metrics.ondevice_word_count, 180);
        assert_eq!(metrics.primary_latency_ms, 1500);
        assert_eq!(metrics.ondevice_latency_ms, 3000);
    }

    #[test]
    fn test_csv_header_format() {
        let header = "timestamp_utc,session_id,primary_word_count,ondevice_word_count,primary_latency_ms,ondevice_latency_ms,primary_section_count,ondevice_section_count";
        let fields: Vec<&str> = header.split(',').collect();
        assert_eq!(fields.len(), 8);
        assert_eq!(fields[0], "timestamp_utc");
        assert_eq!(fields[1], "session_id");
        assert_eq!(fields[7], "ondevice_section_count");
    }
}
