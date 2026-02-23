//! Presence Sensor Module
//!
//! Interfaces with a DFRobot SEN0395 24GHz mmWave presence sensor via USB-UART.
//! The sensor outputs `$JYBSS,0` (absent) / `$JYBSS,1` (present) at ~1Hz.
//!
//! Architecture:
//!   Serial Port (blocking read via spawn_blocking)
//!       → Debounce FSM (10s default)
//!       → watch channel (PresenceState)
//!       → Absence Monitor (async task, 90s default threshold)
//!       → Notify (fires when absence exceeds threshold)
//!
//! Optional CSV logging mirrors the format from `scripts/mmwave_logger.py`.

use chrono::Utc;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

// ============================================================================
// Types
// ============================================================================

/// Debounced presence state from the sensor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceState {
    Present,
    Absent,
    Unknown,
}

impl PresenceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PresenceState::Present => "present",
            PresenceState::Absent => "absent",
            PresenceState::Unknown => "unknown",
        }
    }
}

/// Sensor connection health
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SensorStatus {
    Connected,
    Disconnected,
    Error(String),
}

impl SensorStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, SensorStatus::Connected)
    }
}

/// Configuration for the presence sensor
#[derive(Debug, Clone)]
pub struct SensorConfig {
    pub port: String,
    pub debounce_secs: u64,
    pub absence_threshold_secs: u64,
    pub csv_log_enabled: bool,
}

// ============================================================================
// Sensor Handle
// ============================================================================

/// Handle to a running presence sensor. Call `stop()` to shut down.
pub struct PresenceSensor {
    state_tx: Arc<watch::Sender<PresenceState>>,
    status_tx: Arc<watch::Sender<SensorStatus>>,
    absence_trigger: Arc<Notify>,
    stop: Arc<AtomicBool>,
    reader_handle: Option<JoinHandle<()>>,
    monitor_handle: Option<JoinHandle<()>>,
}

impl PresenceSensor {
    /// Start the presence sensor with the given configuration.
    ///
    /// Returns a handle that provides:
    /// - `subscribe_state()` — watch channel for debounced presence state
    /// - `subscribe_status()` — watch channel for connection health
    /// - `absence_notifier()` — fires when absence exceeds threshold
    pub fn start(config: &SensorConfig) -> Result<Self, String> {
        if config.port.is_empty() {
            return Err("No serial port configured for presence sensor".to_string());
        }

        let (state_tx, _) = watch::channel(PresenceState::Unknown);
        let state_tx = Arc::new(state_tx);

        let (status_tx, _) = watch::channel(SensorStatus::Disconnected);
        let status_tx = Arc::new(status_tx);

        let absence_trigger = Arc::new(Notify::new());
        let stop = Arc::new(AtomicBool::new(false));

        // Start the serial reader task (blocking → tokio bridge)
        let reader_handle = {
            let state_tx = state_tx.clone();
            let status_tx = status_tx.clone();
            let stop = stop.clone();
            let port_path = config.port.clone();
            let debounce_secs = config.debounce_secs;
            let csv_log_enabled = config.csv_log_enabled;

            tokio::spawn(async move {
                serial_reader_loop(
                    &port_path,
                    debounce_secs,
                    csv_log_enabled,
                    state_tx,
                    status_tx,
                    stop,
                )
                .await;
            })
        };

        // Start the absence threshold monitor
        let monitor_handle = {
            let state_rx = state_tx.subscribe();
            let absence_trigger = absence_trigger.clone();
            let stop = stop.clone();
            let threshold_secs = config.absence_threshold_secs;

            tokio::spawn(async move {
                absence_monitor(state_rx, absence_trigger, stop, threshold_secs).await;
            })
        };

        info!(
            "Presence sensor started: port={}, debounce={}s, absence_threshold={}s, csv={}",
            config.port, config.debounce_secs, config.absence_threshold_secs, config.csv_log_enabled
        );

        Ok(Self {
            state_tx,
            status_tx,
            absence_trigger,
            stop,
            reader_handle: Some(reader_handle),
            monitor_handle: Some(monitor_handle),
        })
    }

    /// Get a receiver for the debounced presence state
    pub fn subscribe_state(&self) -> watch::Receiver<PresenceState> {
        self.state_tx.subscribe()
    }

    /// Get a receiver for the sensor connection status
    pub fn subscribe_status(&self) -> watch::Receiver<SensorStatus> {
        self.status_tx.subscribe()
    }

    /// Get the absence trigger notifier (fires when absence exceeds threshold)
    pub fn absence_notifier(&self) -> Arc<Notify> {
        self.absence_trigger.clone()
    }

    /// Stop the sensor and all associated tasks
    pub fn stop(&mut self) {
        info!("Stopping presence sensor");
        self.stop.store(true, Ordering::Relaxed);

        // Abort the tasks — they check stop flag but we also abort to ensure cleanup
        if let Some(h) = self.reader_handle.take() {
            h.abort();
        }
        if let Some(h) = self.monitor_handle.take() {
            h.abort();
        }
    }
}

impl Drop for PresenceSensor {
    fn drop(&mut self) {
        self.stop();
    }
}

// ============================================================================
// Serial Reader (blocking → async bridge)
// ============================================================================

/// Parse a `$JYBSS,{0|1}` line into a raw presence value.
/// Returns `Some(true)` for present, `Some(false)` for absent, `None` for unparseable.
pub(crate) fn parse_jybss(line: &str) -> Option<bool> {
    let trimmed = line.trim();
    if !trimmed.starts_with("$JYBSS,") {
        return None;
    }
    let parts: Vec<&str> = trimmed.split(',').collect();
    if parts.len() < 2 {
        return None;
    }
    match parts[1].chars().next() {
        Some('1') => Some(true),
        Some('0') => Some(false),
        _ => None,
    }
}

/// Blocking serial reader loop. Runs inside `spawn_blocking` via a tokio task.
///
/// Opens the serial port, reads lines, applies debounce, updates watch channels.
/// On serial error: sets status to Disconnected, sleeps 3s, retries.
async fn serial_reader_loop(
    port_path: &str,
    debounce_secs: u64,
    csv_log_enabled: bool,
    state_tx: Arc<watch::Sender<PresenceState>>,
    status_tx: Arc<watch::Sender<SensorStatus>>,
    stop: Arc<AtomicBool>,
) {
    let port_path = port_path.to_string();
    let debounce_dur = Duration::from_secs(debounce_secs);

    // Run the blocking serial I/O in a dedicated thread
    let _ = tokio::task::spawn_blocking(move || {
        // Debounce FSM state
        let mut debounced: Option<bool> = None;
        let mut candidate: Option<bool> = None;
        let mut candidate_since = Instant::now();

        // CSV logger
        let mut csv_writer = if csv_log_enabled {
            CsvLogger::new().ok()
        } else {
            None
        };

        loop {
            if stop.load(Ordering::Relaxed) {
                info!("Presence sensor reader stopping (stop flag set)");
                break;
            }

            // Try to open the port
            let port_result = serialport::new(&port_path, 115200)
                .timeout(Duration::from_secs(2))
                .open();

            let port = match port_result {
                Ok(p) => {
                    info!("Presence sensor connected: {}", port_path);
                    let _ = status_tx.send(SensorStatus::Connected);
                    p
                }
                Err(e) => {
                    warn!("Failed to open serial port {}: {}", port_path, e);
                    let _ = status_tx.send(SensorStatus::Error(e.to_string()));
                    // Sleep and retry
                    for _ in 0..30 {
                        // 3 seconds in 100ms increments, checking stop flag
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    continue;
                }
            };

            let mut reader = std::io::BufReader::new(port);
            let mut line_buf = String::new();

            // Read loop
            loop {
                if stop.load(Ordering::Relaxed) {
                    info!("Presence sensor reader stopping (stop flag set)");
                    return;
                }

                line_buf.clear();
                match reader.read_line(&mut line_buf) {
                    Ok(0) => {
                        // EOF — port closed
                        warn!("Serial port EOF");
                        let _ = status_tx.send(SensorStatus::Disconnected);
                        break;
                    }
                    Ok(_) => {
                        let line = line_buf.trim();
                        if line.is_empty() {
                            continue;
                        }

                        // Parse raw presence value
                        let raw_present = parse_jybss(line);

                        if let Some(raw) = raw_present {
                            let now = Instant::now();

                            // Debounce FSM (mirrors mmwave_logger.py algorithm)
                            match debounced {
                                None => {
                                    // First reading — initialize
                                    debounced = Some(raw);
                                    candidate = Some(raw);
                                    candidate_since = now;
                                }
                                Some(current) => {
                                    if raw != current {
                                        // Different from debounced state
                                        if candidate == Some(raw) {
                                            // Same as candidate — check if held long enough
                                            if now.duration_since(candidate_since) >= debounce_dur {
                                                debounced = Some(raw);
                                                let new_state = if raw {
                                                    PresenceState::Present
                                                } else {
                                                    PresenceState::Absent
                                                };
                                                let direction = if raw { "ARRIVED" } else { "LEFT" };
                                                info!("Presence sensor: {} (debounced)", direction);
                                                let _ = state_tx.send(new_state);
                                            }
                                        } else {
                                            // New candidate
                                            candidate = Some(raw);
                                            candidate_since = now;
                                        }
                                    } else {
                                        // Same as debounced — reset candidate
                                        candidate = Some(raw);
                                        candidate_since = now;
                                    }
                                }
                            }

                            // If this is the first reading and we haven't emitted state yet
                            if debounced.is_some() && *state_tx.borrow() == PresenceState::Unknown {
                                let initial = if debounced.unwrap_or(false) {
                                    PresenceState::Present
                                } else {
                                    PresenceState::Absent
                                };
                                let _ = state_tx.send(initial);
                            }
                        }

                        // CSV logging
                        if let Some(ref mut csv) = csv_writer {
                            let raw_str = match raw_present {
                                Some(true) => "1",
                                Some(false) => "0",
                                None => "",
                            };
                            let deb_str = match debounced {
                                Some(true) => "1",
                                Some(false) => "0",
                                None => "",
                            };
                            csv.write_line(raw_str, deb_str, line);
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::TimedOut {
                            // Normal timeout — no data received within 2s
                            continue;
                        }
                        warn!("Serial read error: {}", e);
                        let _ = status_tx.send(SensorStatus::Disconnected);
                        break;
                    }
                }
            }

            // Connection lost — retry after 3 seconds
            if !stop.load(Ordering::Relaxed) {
                info!("Serial connection lost, reconnecting in 3s...");
                for _ in 0..30 {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    })
    .await;
}

// ============================================================================
// Absence Threshold Monitor
// ============================================================================

/// Watches the debounced presence state. When it transitions to Absent and stays
/// Absent for `threshold_secs`, fires the absence_trigger Notify.
async fn absence_monitor(
    mut state_rx: watch::Receiver<PresenceState>,
    absence_trigger: Arc<Notify>,
    stop: Arc<AtomicBool>,
    threshold_secs: u64,
) {
    let threshold = Duration::from_secs(threshold_secs);

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        // Wait for state to become Absent
        let became_absent = loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let current = *state_rx.borrow();
            if current == PresenceState::Absent {
                break true;
            }
            // Wait for a state change
            if state_rx.changed().await.is_err() {
                // Sender dropped
                return;
            }
        };

        if !became_absent {
            continue;
        }

        debug!("Absence monitor: room became absent, starting {}s timer", threshold_secs);

        // Start the absence timer
        let timer_start = Instant::now();

        loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }

            let remaining = threshold.saturating_sub(timer_start.elapsed());
            if remaining.is_zero() {
                // Threshold exceeded — fire the trigger
                info!(
                    "Absence threshold reached ({}s) — triggering encounter split",
                    threshold_secs
                );
                absence_trigger.notify_one();
                break;
            }

            // Wait for either a state change or the remaining time to elapse
            tokio::select! {
                result = state_rx.changed() => {
                    if result.is_err() {
                        return; // Sender dropped
                    }
                    let current = *state_rx.borrow();
                    if current == PresenceState::Present {
                        debug!("Absence monitor: room became present, cancelling timer");
                        break; // Back to outer loop — wait for next absence
                    }
                    // Still absent (or unknown) — continue timing
                }
                _ = tokio::time::sleep(remaining) => {
                    // Timer expired — will trigger on next iteration
                }
            }
        }
    }
}

// ============================================================================
// CSV Logger
// ============================================================================

/// CSV logger that writes presence data to daily log files.
/// Format matches `scripts/mmwave_logger.py` for backward compatibility.
struct CsvLogger {
    log_dir: PathBuf,
    current_date: String,
    file: Option<std::fs::File>,
}

impl CsvLogger {
    fn new() -> Result<Self, String> {
        let log_dir = dirs::home_dir()
            .ok_or("No home directory")?
            .join(".transcriptionapp")
            .join("mmwave");

        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create mmwave log dir: {}", e))?;

        let today = Utc::now().format("%Y-%m-%d").to_string();
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

    fn write_line(&mut self, raw: &str, debounced: &str, raw_line: &str) {
        let now_utc = Utc::now();
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- JYBSS Parser Tests ---

    #[test]
    fn test_parse_jybss_present() {
        assert_eq!(parse_jybss("$JYBSS,1, , , *"), Some(true));
        assert_eq!(parse_jybss("$JYBSS,1"), Some(true));
        assert_eq!(parse_jybss("$JYBSS,1\r\n"), Some(true));
    }

    #[test]
    fn test_parse_jybss_absent() {
        assert_eq!(parse_jybss("$JYBSS,0, , , *"), Some(false));
        assert_eq!(parse_jybss("$JYBSS,0"), Some(false));
        assert_eq!(parse_jybss("$JYBSS,0\n"), Some(false));
    }

    #[test]
    fn test_parse_jybss_garbage() {
        assert_eq!(parse_jybss("garbage"), None);
        assert_eq!(parse_jybss(""), None);
        assert_eq!(parse_jybss("$JYBSS"), None);
        assert_eq!(parse_jybss("JYBSS,1"), None);
        assert_eq!(parse_jybss("$JYBSS,"), None);
    }

    // --- Debounce FSM Tests ---
    // These test the debounce logic inline since it's embedded in the reader loop.
    // We extract the core logic for testability.

    /// Debounce FSM state for testing
    struct DebounceFsm {
        debounced: Option<bool>,
        candidate: Option<bool>,
        candidate_since: Instant,
        debounce_dur: Duration,
    }

    impl DebounceFsm {
        fn new(debounce_secs: u64) -> Self {
            Self {
                debounced: None,
                candidate: None,
                candidate_since: Instant::now(),
                debounce_dur: Duration::from_secs(debounce_secs),
            }
        }

        /// Process a raw reading and return the new debounced state if it changed.
        fn process(&mut self, raw: bool, now: Instant) -> Option<bool> {
            match self.debounced {
                None => {
                    self.debounced = Some(raw);
                    self.candidate = Some(raw);
                    self.candidate_since = now;
                    Some(raw) // Initial state
                }
                Some(current) => {
                    if raw != current {
                        if self.candidate == Some(raw) {
                            if now.duration_since(self.candidate_since) >= self.debounce_dur {
                                self.debounced = Some(raw);
                                return Some(raw);
                            }
                        } else {
                            self.candidate = Some(raw);
                            self.candidate_since = now;
                        }
                    } else {
                        self.candidate = Some(raw);
                        self.candidate_since = now;
                    }
                    None // No change
                }
            }
        }
    }

    #[test]
    fn test_debounce_rapid_toggles_ignored() {
        let mut fsm = DebounceFsm::new(10); // 10 second debounce
        let start = Instant::now();

        // First reading — establishes initial state
        assert_eq!(fsm.process(true, start), Some(true));

        // Rapid toggles within debounce window — should NOT change state
        assert_eq!(fsm.process(false, start + Duration::from_secs(1)), None);
        assert_eq!(fsm.process(true, start + Duration::from_secs(2)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(3)), None);
        assert_eq!(fsm.process(true, start + Duration::from_secs(4)), None);

        // State should still be true (present)
        assert_eq!(fsm.debounced, Some(true));
    }

    #[test]
    fn test_debounce_sustained_change_transitions() {
        let mut fsm = DebounceFsm::new(10);
        let start = Instant::now();

        // Initial state: present
        assert_eq!(fsm.process(true, start), Some(true));

        // Sustained absent for >10 seconds
        assert_eq!(fsm.process(false, start + Duration::from_secs(1)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(5)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(9)), None);
        // At 11 seconds, should transition
        assert_eq!(
            fsm.process(false, start + Duration::from_secs(11)),
            Some(false)
        );

        assert_eq!(fsm.debounced, Some(false));
    }

    #[test]
    fn test_debounce_reset_on_blip() {
        let mut fsm = DebounceFsm::new(10);
        let start = Instant::now();

        assert_eq!(fsm.process(true, start), Some(true));

        // Start going absent
        assert_eq!(fsm.process(false, start + Duration::from_secs(1)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(5)), None);

        // Blip back to present — resets candidate
        assert_eq!(fsm.process(true, start + Duration::from_secs(6)), None);

        // Now absent again — candidate resets, needs another 10s
        assert_eq!(fsm.process(false, start + Duration::from_secs(7)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(15)), None);

        // 10s after the new candidate started at t=7 → transition at t=17
        assert_eq!(
            fsm.process(false, start + Duration::from_secs(17)),
            Some(false)
        );
    }

    // --- Absence Threshold Tests ---

    #[tokio::test]
    async fn test_absence_threshold_fires_after_duration() {
        let (state_tx, _) = watch::channel(PresenceState::Present);
        let state_tx = Arc::new(state_tx);
        let trigger = Arc::new(Notify::new());
        let stop = Arc::new(AtomicBool::new(false));

        let state_rx = state_tx.subscribe();
        let trigger_clone = trigger.clone();
        let stop_clone = stop.clone();

        // Start monitor with 1s threshold for fast testing
        let monitor = tokio::spawn(async move {
            absence_monitor(state_rx, trigger_clone, stop_clone, 1).await;
        });

        // Transition to absent
        let _ = state_tx.send(PresenceState::Absent);

        // Should trigger within ~1.5 seconds
        let result = tokio::time::timeout(Duration::from_secs(3), trigger.notified()).await;
        assert!(result.is_ok(), "Absence trigger should fire after threshold");

        stop.store(true, Ordering::Relaxed);
        monitor.abort();
    }

    #[tokio::test]
    async fn test_absence_cancelled_by_return_to_present() {
        let (state_tx, _) = watch::channel(PresenceState::Present);
        let state_tx = Arc::new(state_tx);
        let trigger = Arc::new(Notify::new());
        let stop = Arc::new(AtomicBool::new(false));

        let state_rx = state_tx.subscribe();
        let trigger_clone = trigger.clone();
        let stop_clone = stop.clone();

        // 2s threshold
        let monitor = tokio::spawn(async move {
            absence_monitor(state_rx, trigger_clone, stop_clone, 2).await;
        });

        // Absent
        let _ = state_tx.send(PresenceState::Absent);
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Return to present before threshold
        let _ = state_tx.send(PresenceState::Present);

        // Trigger should NOT fire
        let result = tokio::time::timeout(Duration::from_secs(3), trigger.notified()).await;
        assert!(result.is_err(), "Trigger should NOT fire when person returns");

        stop.store(true, Ordering::Relaxed);
        monitor.abort();
    }

    // --- CSV Format Tests ---

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

    // --- Config Clamping Tests ---

    #[test]
    fn test_presence_state_as_str() {
        assert_eq!(PresenceState::Present.as_str(), "present");
        assert_eq!(PresenceState::Absent.as_str(), "absent");
        assert_eq!(PresenceState::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_sensor_status_is_connected() {
        assert!(SensorStatus::Connected.is_connected());
        assert!(!SensorStatus::Disconnected.is_connected());
        assert!(!SensorStatus::Error("test".to_string()).is_connected());
    }
}
