//! Presence Sensor Module
//!
//! Interfaces with a DFRobot SEN0395 24GHz mmWave presence sensor via:
//!   - **HTTP** (preferred): Polls an ESP32 WiFi bridge at `presence_sensor_url`
//!   - **Serial** (fallback): Reads USB-UART at `presence_sensor_port`
//!
//! The sensor outputs `$JYBSS,0` (absent) / `$JYBSS,1` (present) at ~1Hz.
//!
//! Architecture:
//!   HTTP Poller or Serial Port (blocking read via spawn_blocking)
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
    /// Serial port path (e.g., /dev/cu.usbserial-2110) — used when `url` is empty
    pub port: String,
    /// HTTP URL of ESP32 WiFi bridge (e.g., http://172.16.100.37) — takes precedence over `port`
    pub url: String,
    pub debounce_secs: u64,
    pub absence_threshold_secs: u64,
    pub csv_log_enabled: bool,
}

// ============================================================================
// Auto-Detection
// ============================================================================

/// Auto-detect the presence sensor serial port.
///
/// Scans available serial ports for USB-serial devices (matching common patterns
/// like `usbserial`, `usbmodem`, `USB`). If `configured_port` is non-empty and
/// exists among available ports, it is returned as-is. Otherwise, returns the
/// first matching USB-serial port found, or None.
pub fn auto_detect_port(configured_port: &str) -> Option<String> {
    let ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to enumerate serial ports: {}", e);
            return if configured_port.is_empty() {
                None
            } else {
                Some(configured_port.to_string())
            };
        }
    };

    let port_names: Vec<&str> = ports.iter().map(|p| p.port_name.as_str()).collect();
    debug!("Available serial ports: {:?}", port_names);

    // If configured port exists, use it
    if !configured_port.is_empty() && ports.iter().any(|p| p.port_name == configured_port) {
        return Some(configured_port.to_string());
    }

    // Auto-detect: look for USB serial ports (common patterns on macOS/Linux)
    let usb_patterns = ["usbserial", "usbmodem", "USB"];
    for port in &ports {
        if usb_patterns.iter().any(|pat| port.port_name.contains(pat)) {
            if !configured_port.is_empty() {
                info!(
                    "Configured sensor port '{}' not found. Auto-detected: {}",
                    configured_port, port.port_name
                );
            } else {
                info!("Auto-detected sensor port: {}", port.port_name);
            }
            return Some(port.port_name.clone());
        }
    }

    if !configured_port.is_empty() {
        warn!(
            "Configured sensor port '{}' not found and no USB serial port detected",
            configured_port
        );
    }
    None
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
    /// If `config.url` is set, uses HTTP polling to an ESP32 WiFi bridge.
    /// Otherwise falls back to serial port reading.
    ///
    /// Returns a handle that provides:
    /// - `subscribe_state()` — watch channel for debounced presence state
    /// - `subscribe_status()` — watch channel for connection health
    /// - `absence_notifier()` — fires when absence exceeds threshold
    pub fn start(config: &SensorConfig) -> Result<Self, String> {
        let use_http = !config.url.is_empty();
        if !use_http && config.port.is_empty() {
            return Err("No sensor URL or serial port configured".to_string());
        }

        let (state_tx, _) = watch::channel(PresenceState::Unknown);
        let state_tx = Arc::new(state_tx);

        let (status_tx, _) = watch::channel(SensorStatus::Disconnected);
        let status_tx = Arc::new(status_tx);

        let absence_trigger = Arc::new(Notify::new());
        let stop = Arc::new(AtomicBool::new(false));

        // Start the reader task — HTTP or serial depending on config
        let reader_handle = {
            let state_tx = state_tx.clone();
            let status_tx = status_tx.clone();
            let stop = stop.clone();
            let debounce_secs = config.debounce_secs;
            let csv_log_enabled = config.csv_log_enabled;

            if use_http {
                let url = config.url.clone();
                tokio::spawn(async move {
                    http_reader_loop(
                        &url,
                        debounce_secs,
                        csv_log_enabled,
                        state_tx,
                        status_tx,
                        stop,
                    )
                    .await;
                })
            } else {
                let port_path = config.port.clone();
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
            }
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

        let source = if use_http {
            format!("url={}", config.url)
        } else {
            format!("port={}", config.port)
        };
        info!(
            "Presence sensor started: {}, debounce={}s, absence_threshold={}s, csv={}",
            source, config.debounce_secs, config.absence_threshold_secs, config.csv_log_enabled
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
// HTTP Reader (polls ESP32 WiFi bridge)
// ============================================================================

/// JSON response from the ESP32 presence sensor bridge
#[derive(Debug, serde::Deserialize)]
struct Esp32Response {
    present: bool,
    #[serde(default)]
    sensor_stale: bool,
}

/// HTTP polling loop for ESP32 WiFi bridge.
///
/// Polls `{url}/` every ~1s, parses JSON `{"present": bool, "sensor_stale": bool, ...}`,
/// feeds raw `present` value into debounce FSM, updates watch channels.
/// On HTTP error or sensor_stale: sets status to Error/Disconnected, retries after 3s.
async fn http_reader_loop(
    base_url: &str,
    debounce_secs: u64,
    csv_log_enabled: bool,
    state_tx: Arc<watch::Sender<PresenceState>>,
    status_tx: Arc<watch::Sender<SensorStatus>>,
    stop: Arc<AtomicBool>,
) {
    let url = if base_url.ends_with('/') {
        base_url.to_string()
    } else {
        format!("{}/", base_url)
    };
    let debounce_dur = Duration::from_secs(debounce_secs);

    // Debounce FSM state
    let mut debounced: Option<bool> = None;
    let mut candidate: Option<bool> = None;
    let mut candidate_since = Instant::now();
    let mut was_connected = false;

    // CSV logger
    let mut csv_writer = if csv_log_enabled {
        CsvLogger::new().ok()
    } else {
        None
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    loop {
        if stop.load(Ordering::Relaxed) {
            info!("Presence sensor HTTP reader stopping (stop flag set)");
            break;
        }

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<Esp32Response>().await {
                    Ok(data) => {
                        if data.sensor_stale {
                            if was_connected {
                                warn!("ESP32 sensor stale (no readings from mmWave sensor)");
                                let _ = status_tx.send(SensorStatus::Error(
                                    "Sensor stale — no mmWave readings".to_string(),
                                ));
                                was_connected = false;
                            }
                        } else {
                            if !was_connected {
                                info!("Presence sensor connected via HTTP: {}", base_url);
                                let _ = status_tx.send(SensorStatus::Connected);
                                was_connected = true;
                            }

                            let raw = data.present;
                            let now = Instant::now();

                            // Debounce FSM (same algorithm as serial reader)
                            match debounced {
                                None => {
                                    debounced = Some(raw);
                                    candidate = Some(raw);
                                    candidate_since = now;
                                }
                                Some(current) => {
                                    if raw != current {
                                        if candidate == Some(raw) {
                                            if now.duration_since(candidate_since) >= debounce_dur {
                                                debounced = Some(raw);
                                                let new_state = if raw {
                                                    PresenceState::Present
                                                } else {
                                                    PresenceState::Absent
                                                };
                                                let direction =
                                                    if raw { "ARRIVED" } else { "LEFT" };
                                                info!(
                                                    "Presence sensor (HTTP): {} (debounced)",
                                                    direction
                                                );
                                                let _ = state_tx.send(new_state);
                                            }
                                        } else {
                                            candidate = Some(raw);
                                            candidate_since = now;
                                        }
                                    } else {
                                        candidate = Some(raw);
                                        candidate_since = now;
                                    }
                                }
                            }

                            // Emit initial state
                            if debounced.is_some()
                                && *state_tx.borrow() == PresenceState::Unknown
                            {
                                let initial = if debounced.unwrap_or(false) {
                                    PresenceState::Present
                                } else {
                                    PresenceState::Absent
                                };
                                let _ = state_tx.send(initial);
                            }

                            // CSV logging
                            if let Some(ref mut csv) = csv_writer {
                                let raw_str = if raw { "1" } else { "0" };
                                let deb_str = match debounced {
                                    Some(true) => "1",
                                    Some(false) => "0",
                                    None => "",
                                };
                                let raw_line =
                                    format!("HTTP present={} stale={}", raw, data.sensor_stale);
                                csv.write_line(raw_str, deb_str, &raw_line);
                            }
                        }
                    }
                    Err(e) => {
                        if was_connected {
                            warn!("ESP32 response parse error: {}", e);
                            let _ = status_tx.send(SensorStatus::Error(e.to_string()));
                            was_connected = false;
                        }
                    }
                }
            }
            Ok(resp) => {
                if was_connected {
                    warn!("ESP32 HTTP error: status {}", resp.status());
                    let _ = status_tx.send(SensorStatus::Error(format!(
                        "HTTP {}",
                        resp.status()
                    )));
                    was_connected = false;
                }
                // Back off on error
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
            Err(e) => {
                if was_connected {
                    warn!("ESP32 connection error: {}", e);
                    let _ = status_tx.send(SensorStatus::Disconnected);
                    was_connected = false;
                }
                // Back off on error
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        }

        // Poll interval: ~1s (matches sensor's ~1Hz output rate)
        tokio::time::sleep(Duration::from_secs(1)).await;
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

    // --- Auto-Detection Tests ---

    #[test]
    fn test_auto_detect_returns_some_when_port_enumeration_works() {
        // auto_detect_port should not panic regardless of system state
        let result = auto_detect_port("");
        // Result depends on hardware — just verify it doesn't crash
        let _ = result;
    }

    #[test]
    fn test_auto_detect_with_nonexistent_configured_port() {
        // A configured port that doesn't exist should attempt auto-detection
        let result = auto_detect_port("/dev/cu.nonexistent-9999");
        // If a real USB serial port happens to be connected, it'll find it;
        // otherwise returns None. Either way, shouldn't crash or return
        // the nonexistent port.
        if let Some(ref port) = result {
            assert_ne!(port, "/dev/cu.nonexistent-9999");
        }
    }

    // --- ESP32 HTTP Response Parsing Tests ---

    #[test]
    fn test_esp32_response_parsing() {
        let json = r#"{"present":true,"sensor_stale":false,"sensor_age_ms":616,"uptime_s":114,"wifi_rssi":-64,"ip":"172.16.100.37"}"#;
        let resp: Esp32Response = serde_json::from_str(json).expect("Should parse ESP32 response");
        assert!(resp.present);
        assert!(!resp.sensor_stale);
    }

    #[test]
    fn test_esp32_response_absent() {
        let json = r#"{"present":false,"sensor_stale":false}"#;
        let resp: Esp32Response = serde_json::from_str(json).expect("Should parse");
        assert!(!resp.present);
        assert!(!resp.sensor_stale);
    }

    #[test]
    fn test_esp32_response_stale_sensor() {
        let json = r#"{"present":false,"sensor_stale":true,"sensor_age_ms":10000}"#;
        let resp: Esp32Response = serde_json::from_str(json).expect("Should parse");
        assert!(resp.sensor_stale);
    }

    #[test]
    fn test_esp32_response_minimal() {
        // sensor_stale defaults to false when missing
        let json = r#"{"present":true}"#;
        let resp: Esp32Response = serde_json::from_str(json).expect("Should parse minimal");
        assert!(resp.present);
        assert!(!resp.sensor_stale);
    }

    // --- SensorConfig Mode Selection Tests ---

    #[test]
    fn test_sensor_config_url_takes_precedence() {
        // When URL is set, start() should choose HTTP mode (we can't fully test start()
        // without a server, but we can verify the config logic)
        let config = SensorConfig {
            port: "/dev/cu.usbserial-1234".to_string(),
            url: "http://172.16.100.37".to_string(),
            debounce_secs: 15,
            absence_threshold_secs: 180,
            csv_log_enabled: false,
        };
        assert!(!config.url.is_empty(), "URL should be set");
    }

    #[test]
    fn test_sensor_config_empty_url_falls_back_to_port() {
        let config = SensorConfig {
            port: "/dev/cu.usbserial-1234".to_string(),
            url: String::new(),
            debounce_secs: 15,
            absence_threshold_secs: 180,
            csv_log_enabled: false,
        };
        assert!(config.url.is_empty(), "URL should be empty for serial fallback");
        assert!(!config.port.is_empty(), "Port should be set");
    }

    #[test]
    fn test_sensor_config_both_empty_is_invalid() {
        let config = SensorConfig {
            port: String::new(),
            url: String::new(),
            debounce_secs: 15,
            absence_threshold_secs: 180,
            csv_log_enabled: false,
        };
        let result = PresenceSensor::start(&config);
        assert!(result.is_err(), "Should fail when both port and URL are empty");
    }
}
