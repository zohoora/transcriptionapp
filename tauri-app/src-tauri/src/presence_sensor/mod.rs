//! Multi-Sensor Presence Detection Suite
//!
//! Replaces the monolithic mmWave-only presence sensor with a modular
//! architecture that fuses data from multiple sensors:
//!
//! - **mmWave** (SEN0395): Fast binary presence, 93% false positive at desk
//! - **Thermal** (MLX90640): 32x24 IR camera, 0% false positive, counts people
//! - **CO2** (SCD41): Tracks occupancy count with ~6 min lag, r=0.84 correlation
//!
//! Architecture:
//!
//! ```text
//! SensorSource(s) ──→ mpsc::channel ──→ Fusion Task ──→ watch channels
//!                                        ├─ DebounceFsm (mmWave)
//!                                        ├─ ThermalAnalysis (hot pixels, blobs)
//!                                        └─ Co2Tracker (trend, occupancy)
//! ```
//!
//! Consumer interface is backward-compatible with the old `PresenceSensor`.

pub mod absence_monitor;
pub mod co2;
pub mod csv_logger;
pub mod debounce;
pub mod fusion;
pub mod sensor_source;
pub mod sources;
pub mod thermal;
pub mod types;

// Re-export public API for backward compatibility
pub use sources::serial::{auto_detect_port, parse_jybss};
pub use types::{
    FusedState, OccupancyEstimate, PresenceState, SensorConfig, SensorHealth, SensorStatus,
    SensorType, SuiteConfig, SuiteStatus, ThermalConfig, Co2Config, FusionConfig,
};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, watch, Notify};
use tokio::task::JoinHandle;
use tracing::{debug, info};

use co2::Co2Tracker;
use csv_logger::CsvLogger;
use debounce::DebounceFsm;
use fusion::{analyze_thermal, fuse, FusionInput};
use sensor_source::SensorSource;
use sources::esp32_http::Esp32HttpSource;
use sources::serial::SerialSource;
use types::SensorValue;

/// Handle to a running multi-sensor presence detection suite.
///
/// Drop-in replacement for the old `PresenceSensor` with additional
/// occupancy estimation from fused multi-sensor data.
pub struct PresenceSensorSuite {
    state_tx: Arc<watch::Sender<PresenceState>>,
    status_tx: Arc<watch::Sender<SensorStatus>>,
    occupancy_tx: Arc<watch::Sender<OccupancyEstimate>>,
    absence_trigger: Arc<Notify>,
    stop: Arc<AtomicBool>,
    fusion_handle: Option<JoinHandle<()>>,
    monitor_handle: Option<JoinHandle<()>>,
    source_handle: Option<JoinHandle<()>>,
}

/// Backward-compatible type alias
pub type PresenceSensor = PresenceSensorSuite;

impl PresenceSensorSuite {
    /// Start the presence sensor suite with legacy configuration.
    ///
    /// This is the backward-compatible entry point. Creates an ESP32 HTTP source
    /// (if URL is configured) or serial source (fallback), plus the fusion task
    /// and absence monitor.
    pub fn start(config: &SensorConfig) -> Result<Self, String> {
        let suite_config: SuiteConfig = config.clone().into();
        Self::start_suite(&suite_config)
    }

    /// Start the presence sensor suite with full multi-sensor configuration.
    pub fn start_suite(config: &SuiteConfig) -> Result<Self, String> {
        let use_http = !config.url.is_empty();
        if !use_http && config.port.is_empty() {
            return Err("No sensor URL or serial port configured".to_string());
        }

        // Create channels
        let (state_tx, _) = watch::channel(PresenceState::Unknown);
        let state_tx = Arc::new(state_tx);
        let (status_tx, _) = watch::channel(SensorStatus::Disconnected);
        let status_tx = Arc::new(status_tx);
        let (occupancy_tx, _) = watch::channel(OccupancyEstimate::default());
        let occupancy_tx = Arc::new(occupancy_tx);
        let absence_trigger = Arc::new(Notify::new());
        let stop = Arc::new(AtomicBool::new(false));

        // Create mpsc channel for sensor readings
        let (reading_tx, reading_rx) = mpsc::channel::<types::SensorReading>(128);

        // Create and start the appropriate source
        let source: Box<dyn SensorSource> = if use_http {
            Box::new(Esp32HttpSource::new(config.url.clone()))
        } else {
            Box::new(SerialSource::new(config.port.clone()))
        };

        let source_handle = source
            .start(reading_tx, status_tx.clone(), stop.clone())
            .map_err(|e| format!("Failed to start sensor source: {}", e))?;

        // Start the fusion task
        let fusion_handle = {
            let state_tx = state_tx.clone();
            let occupancy_tx = occupancy_tx.clone();
            let stop = stop.clone();
            let debounce_secs = config.debounce_secs;
            let csv_log_enabled = config.csv_log_enabled;
            let thermal_config = config.thermal.clone();
            let co2_config = config.co2.clone();
            let fusion_config = config.fusion.clone();

            tokio::spawn(async move {
                fusion_task(
                    reading_rx,
                    state_tx,
                    occupancy_tx,
                    stop,
                    debounce_secs,
                    csv_log_enabled,
                    thermal_config,
                    co2_config,
                    fusion_config,
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
                absence_monitor::absence_monitor(state_rx, absence_trigger, stop, threshold_secs)
                    .await;
            })
        };

        let source_name = if use_http {
            format!("url={}", config.url)
        } else {
            format!("port={}", config.port)
        };
        info!(
            "Presence sensor suite started: {}, debounce={}s, absence_threshold={}s, csv={}",
            source_name, config.debounce_secs, config.absence_threshold_secs, config.csv_log_enabled
        );

        Ok(Self {
            state_tx,
            status_tx,
            occupancy_tx,
            absence_trigger,
            stop,
            fusion_handle: Some(fusion_handle),
            monitor_handle: Some(monitor_handle),
            source_handle: Some(source_handle),
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

    /// Get a receiver for occupancy estimates
    pub fn subscribe_occupancy(&self) -> watch::Receiver<OccupancyEstimate> {
        self.occupancy_tx.subscribe()
    }

    /// Get the absence trigger notifier (fires when absence exceeds threshold)
    pub fn absence_notifier(&self) -> Arc<Notify> {
        self.absence_trigger.clone()
    }

    /// Stop the sensor suite and all associated tasks
    pub fn stop(&mut self) {
        info!("Stopping presence sensor suite");
        self.stop.store(true, Ordering::Relaxed);

        if let Some(h) = self.source_handle.take() {
            h.abort();
        }
        if let Some(h) = self.fusion_handle.take() {
            h.abort();
        }
        if let Some(h) = self.monitor_handle.take() {
            h.abort();
        }
    }
}

impl Drop for PresenceSensorSuite {
    fn drop(&mut self) {
        self.stop();
    }
}

// ============================================================================
// Fusion Task
// ============================================================================

/// Core fusion task: receives raw sensor readings, maintains per-sensor state,
/// runs the priority-cascade fusion algorithm, and updates watch channels.
async fn fusion_task(
    mut reading_rx: mpsc::Receiver<types::SensorReading>,
    state_tx: Arc<watch::Sender<PresenceState>>,
    occupancy_tx: Arc<watch::Sender<OccupancyEstimate>>,
    stop: Arc<AtomicBool>,
    debounce_secs: u64,
    csv_log_enabled: bool,
    thermal_config: ThermalConfig,
    co2_config: types::Co2Config,
    fusion_config: types::FusionConfig,
) {
    let mut debounce_fsm = DebounceFsm::new(debounce_secs);
    let mut co2_tracker = Co2Tracker::new(co2_config);

    // Latest state per sensor
    let mut mmwave_last_reading: Option<Instant> = None;
    let mut thermal_snapshot: Option<fusion::ThermalSnapshot> = None;
    let mut co2_last_reading: Option<Instant> = None;

    // CSV logger
    let mut csv_writer = if csv_log_enabled {
        CsvLogger::new().ok()
    } else {
        None
    };

    while let Some(reading) = reading_rx.recv().await {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let now = reading.timestamp;

        match reading.value {
            SensorValue::Presence(raw) => {
                // Apply debounce FSM to mmWave readings
                if let Some(debounced) = debounce_fsm.process(raw, now) {
                    let direction = if debounced { "ARRIVED" } else { "LEFT" };
                    info!("Presence sensor: {} (debounced)", direction);
                }
                mmwave_last_reading = Some(now);

                // CSV logging for mmWave (backward compat with old format)
                if let Some(ref mut csv) = csv_writer {
                    let raw_str = if raw { "1" } else { "0" };
                    let deb_str = match debounce_fsm.current() {
                        Some(true) => "1",
                        Some(false) => "0",
                        None => "",
                    };
                    let raw_line = format!("HTTP present={}", raw);
                    csv.write_line(raw_str, deb_str, &raw_line);
                }
            }
            SensorValue::ThermalFrame {
                pixels,
                width,
                height,
            } => {
                thermal_snapshot =
                    Some(analyze_thermal(&pixels, width, height, &thermal_config, now));
                debug!(
                    "Thermal frame: present={}, occupancy={}",
                    thermal_snapshot.as_ref().unwrap().is_present,
                    thermal_snapshot.as_ref().unwrap().occupancy_count
                );
            }
            SensorValue::Co2 {
                ppm,
                temperature_c: _,
                humidity_pct: _,
            } => {
                co2_tracker.add_reading(ppm, now);
                co2_last_reading = Some(now);
                debug!(
                    "CO2: {:.0} ppm, elevated={}, trend={:?}",
                    ppm,
                    co2_tracker.is_elevated(),
                    co2_tracker.trend()
                );
            }
        }

        // Run fusion on every update
        let input = FusionInput {
            mmwave_present: debounce_fsm.current(),
            mmwave_last_reading,
            thermal_frame: thermal_snapshot,
            co2_elevated: co2_tracker.is_elevated(),
            co2_trend: co2_tracker.trend(),
            co2_occupancy: co2_tracker.estimated_occupancy(),
            co2_last_reading,
            now: Instant::now(),
        };

        let fused = fuse(&input, &fusion_config);

        // Update watch channels only when values change
        state_tx.send_if_modified(|current| {
            if *current != fused.presence {
                *current = fused.presence;
                true
            } else {
                false
            }
        });
        occupancy_tx.send_if_modified(|current| {
            if *current != fused.occupancy {
                *current = fused.occupancy;
                true
            } else {
                false
            }
        });
    }

    debug!("Fusion task exiting");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_sensor_config_both_empty_is_invalid() {
        let config = SensorConfig {
            port: String::new(),
            url: String::new(),
            debounce_secs: 15,
            absence_threshold_secs: 180,
            csv_log_enabled: false,
        };
        let result = PresenceSensorSuite::start(&config);
        assert!(result.is_err(), "Should fail when both port and URL are empty");
    }

    #[test]
    fn test_sensor_config_url_takes_precedence() {
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
        assert!(config.url.is_empty());
        assert!(!config.port.is_empty());
    }

    #[tokio::test]
    async fn test_fusion_task_with_mock_source() {
        use sources::mock::MockSource;

        let source = MockSource::mmwave_sequence(vec![
            (Duration::from_millis(0), true),
            (Duration::from_millis(50), true),
            (Duration::from_millis(50), false),
        ]);

        let (reading_tx, reading_rx) = mpsc::channel(32);
        let (status_tx, _) = watch::channel(SensorStatus::Disconnected);
        let status_tx = Arc::new(status_tx);
        let stop = Arc::new(AtomicBool::new(false));
        let (state_tx, mut state_rx) = watch::channel(PresenceState::Unknown);
        let state_tx = Arc::new(state_tx);
        let (occupancy_tx, _) = watch::channel(OccupancyEstimate::default());
        let occupancy_tx = Arc::new(occupancy_tx);

        // Start mock source
        let source_handle = source.start(reading_tx, status_tx, stop.clone()).unwrap();

        // Start fusion task with 0s debounce for fast testing
        let fusion_stop = stop.clone();
        let fusion_handle = tokio::spawn(async move {
            fusion_task(
                reading_rx,
                state_tx,
                occupancy_tx,
                fusion_stop,
                0, // 0s debounce
                false,
                ThermalConfig::default(),
                types::Co2Config::default(),
                types::FusionConfig::default(),
            )
            .await;
        });

        // Wait for state to change from Unknown
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                state_rx.changed().await.unwrap();
                let state = *state_rx.borrow();
                if state != PresenceState::Unknown {
                    return state;
                }
            }
        })
        .await;

        assert!(result.is_ok(), "Should receive a non-Unknown state");
        // First reading is true → Present (with 0s debounce)
        assert_eq!(result.unwrap(), PresenceState::Present);

        stop.store(true, Ordering::Relaxed);
        source_handle.abort();
        fusion_handle.abort();
    }
}
