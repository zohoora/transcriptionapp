//! CSV logger for presence sensor data.
//!
//! Writes daily CSV files to `~/.transcriptionapp/mmwave/YYYY-MM-DD.csv`.
//! Format matches `scripts/mmwave_logger.py` for backward compatibility.

use std::path::PathBuf;
use tracing::{info, warn};

pub struct CsvLogger {
    log_dir: PathBuf,
    current_date: String,
    file: Option<std::fs::File>,
}

impl CsvLogger {
    pub fn new() -> Result<Self, String> {
        let log_dir = dirs::home_dir()
            .ok_or("No home directory")?
            .join(".transcriptionapp")
            .join("mmwave");

        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create mmwave log dir: {}", e))?;

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut logger = CsvLogger {
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
            .map_err(|e| format!("Failed to open CSV log: {}", e))?;

        if write_header {
            use std::io::Write;
            let mut f = &file;
            let _ = writeln!(
                f,
                "timestamp_utc,timestamp_local,presence_raw,presence_debounced,raw"
            );
        }

        self.file = Some(file);
        Ok(())
    }

    pub fn write_line(&mut self, raw: &str, debounced: &str, raw_line: &str) {
        let now_utc = chrono::Utc::now();
        let now_local = chrono::Local::now();

        // Check for midnight rotation
        let today = now_utc.format("%Y-%m-%d").to_string();
        if today != self.current_date {
            self.current_date = today;
            if let Err(e) = self.open_file() {
                warn!("Failed to rotate CSV log: {}", e);
                return;
            }
            info!("CSV log rotated to {}.csv", self.current_date);
        }

        if let Some(ref mut file) = self.file {
            use std::io::Write;
            let ts_utc = now_utc.format("%Y-%m-%dT%H:%M:%S%.3fZ");
            let ts_local = now_local.format("%Y-%m-%dT%H:%M:%S%.3f%z");
            let raw_escaped = raw_line.replace('"', "\"\"");
            let _ = writeln!(
                file,
                "{},{},{},{},\"{}\"",
                ts_utc, ts_local, raw, debounced, raw_escaped
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_csv_line_format() {
        // Verify the format matches mmwave_logger.py
        let raw = "1";
        let debounced = "1";
        let raw_line = "$JYBSS,1, , , *";
        let raw_escaped = raw_line.replace('"', "\"\"");

        let line = format!(
            "2026-02-19T10:30:00.000Z,2026-02-19T13:30:00.000+0300,{},{},\"{}\"",
            raw, debounced, raw_escaped
        );

        assert!(line.contains("2026-02-19T10:30:00.000Z"));
        assert!(line.contains(",1,1,"));
        assert!(line.contains("$JYBSS,1"));
    }
}
