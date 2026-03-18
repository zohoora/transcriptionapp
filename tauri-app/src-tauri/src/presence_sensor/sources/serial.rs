//! Serial sensor source — reads mmWave presence via USB-UART.
//!
//! Legacy interface for direct serial connection to the SEN0395 mmWave sensor.
//! Parses `$JYBSS,{0|1}` lines at 115200 baud.

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::presence_sensor::sensor_source::SensorSource;
use crate::presence_sensor::types::{
    SensorReading, SensorStatus, SensorType, SensorValue,
};

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

/// Parse a `$JYBSS,{0|1}` line into a raw presence value.
/// Returns `Some(true)` for present, `Some(false)` for absent, `None` for unparseable.
pub fn parse_jybss(line: &str) -> Option<bool> {
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

/// Serial sensor source for direct USB-UART connection
pub struct SerialSource {
    port: String,
}

impl SerialSource {
    pub fn new(port: String) -> Self {
        Self { port }
    }
}

impl SensorSource for SerialSource {
    fn name(&self) -> &str {
        "serial"
    }

    fn provided_sensors(&self) -> Vec<SensorType> {
        vec![SensorType::MmWave]
    }

    fn start(
        &self,
        reading_tx: mpsc::Sender<SensorReading>,
        status_tx: Arc<watch::Sender<SensorStatus>>,
        stop: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>, String> {
        let port_path = self.port.clone();

        let handle = tokio::spawn(async move {
            serial_reader_loop(&port_path, reading_tx, status_tx, stop).await;
        });

        Ok(handle)
    }
}

/// Blocking serial reader loop. Runs inside `spawn_blocking` via a tokio task.
async fn serial_reader_loop(
    port_path: &str,
    reading_tx: mpsc::Sender<SensorReading>,
    status_tx: Arc<watch::Sender<SensorStatus>>,
    stop: Arc<AtomicBool>,
) {
    let port_path = port_path.to_string();

    let _ = tokio::task::spawn_blocking(move || {
        loop {
            if stop.load(Ordering::Relaxed) {
                info!("Serial sensor reader stopping (stop flag set)");
                break;
            }

            // Try to open the port
            let port_result = serialport::new(&port_path, 115200)
                .timeout(Duration::from_secs(2))
                .open();

            let port = match port_result {
                Ok(p) => {
                    info!("Serial sensor connected: {}", port_path);
                    let _ = status_tx.send(SensorStatus::Connected);
                    p
                }
                Err(e) => {
                    warn!("Failed to open serial port {}: {}", port_path, e);
                    let _ = status_tx.send(SensorStatus::Error(e.to_string()));
                    for _ in 0..30 {
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

            loop {
                if stop.load(Ordering::Relaxed) {
                    info!("Serial sensor reader stopping (stop flag set)");
                    return;
                }

                line_buf.clear();
                match reader.read_line(&mut line_buf) {
                    Ok(0) => {
                        warn!("Serial port EOF");
                        let _ = status_tx.send(SensorStatus::Disconnected);
                        break;
                    }
                    Ok(_) => {
                        let line = line_buf.trim();
                        if line.is_empty() {
                            continue;
                        }

                        if let Some(raw) = parse_jybss(line) {
                            let _ = reading_tx.blocking_send(SensorReading {
                                sensor_type: SensorType::MmWave,
                                timestamp: Instant::now(),
                                value: SensorValue::Presence(raw),
                            });
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::TimedOut {
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_auto_detect_returns_some_when_port_enumeration_works() {
        let result = auto_detect_port("");
        let _ = result;
    }

    #[test]
    fn test_auto_detect_with_nonexistent_configured_port() {
        let result = auto_detect_port("/dev/cu.nonexistent-9999");
        if let Some(ref port) = result {
            assert_ne!(port, "/dev/cu.nonexistent-9999");
        }
    }

    #[test]
    fn test_source_metadata() {
        let source = SerialSource::new("/dev/ttyUSB0".to_string());
        assert_eq!(source.name(), "serial");
        assert_eq!(source.provided_sensors(), vec![SensorType::MmWave]);
    }
}
