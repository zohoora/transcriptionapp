//! Shared types for the multi-sensor presence detection suite.

use std::time::Instant;

// ============================================================================
// Existing types (backward compatible)
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

/// Type alias for the suite-level status (renamed in new API)
pub type SuiteStatus = SensorStatus;

/// Legacy configuration for the presence sensor (backward compatible)
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
// New multi-sensor types
// ============================================================================

/// Types of physical sensors in the multi-sensor suite
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensorType {
    MmWave,
    Thermal,
    Co2,
}

impl SensorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SensorType::MmWave => "mmwave",
            SensorType::Thermal => "thermal",
            SensorType::Co2 => "co2",
        }
    }
}

/// A single reading from a physical sensor
#[derive(Debug)]
pub struct SensorReading {
    pub sensor_type: SensorType,
    pub timestamp: Instant,
    pub value: SensorValue,
}

/// Typed sensor measurement values
#[derive(Debug)]
pub enum SensorValue {
    /// mmWave binary presence (raw, pre-debounce)
    Presence(bool),
    /// MLX90640 thermal camera frame (32x24 = 768 pixels, temperatures in Celsius)
    ThermalFrame {
        pixels: Vec<f32>,
        width: u16,
        height: u16,
    },
    /// SCD41 CO2/environmental readings
    Co2 {
        ppm: f32,
        temperature_c: f32,
        humidity_pct: f32,
    },
}

/// Estimated occupancy from fused sensor data
#[derive(Debug, Clone, PartialEq)]
pub struct OccupancyEstimate {
    /// Number of people detected (None = cannot determine)
    pub count: Option<u8>,
    /// Confidence in the estimate (0.0–1.0)
    pub confidence: f32,
    /// Which sensors contributed to this estimate
    pub contributing_sensors: Vec<SensorType>,
}

impl Default for OccupancyEstimate {
    fn default() -> Self {
        Self {
            count: None,
            confidence: 0.0,
            contributing_sensors: Vec::new(),
        }
    }
}

/// Health status of an individual sensor
#[derive(Debug, Clone)]
pub struct SensorHealth {
    pub sensor_type: SensorType,
    pub last_reading: Option<Instant>,
    pub is_stale: bool,
}

/// Result of fusing all available sensor data
#[derive(Debug, Clone)]
pub struct FusedState {
    pub presence: PresenceState,
    pub occupancy: OccupancyEstimate,
    pub sensor_health: Vec<SensorHealth>,
}

// ============================================================================
// Configuration types
// ============================================================================

/// Configuration for the multi-sensor suite
#[derive(Debug, Clone)]
pub struct SuiteConfig {
    // Legacy fields (backward compat with SensorConfig)
    pub port: String,
    pub url: String,
    pub debounce_secs: u64,
    pub absence_threshold_secs: u64,
    pub csv_log_enabled: bool,
    // Multi-sensor fields
    pub thermal: ThermalConfig,
    pub co2: Co2Config,
    pub fusion: FusionConfig,
}

impl From<SensorConfig> for SuiteConfig {
    fn from(cfg: SensorConfig) -> Self {
        Self {
            port: cfg.port,
            url: cfg.url,
            debounce_secs: cfg.debounce_secs,
            absence_threshold_secs: cfg.absence_threshold_secs,
            csv_log_enabled: cfg.csv_log_enabled,
            thermal: ThermalConfig::default(),
            co2: Co2Config::default(),
            fusion: FusionConfig::default(),
        }
    }
}

/// Thermal analysis configuration
#[derive(Debug, Clone)]
pub struct ThermalConfig {
    /// Temperature threshold for "hot" pixels (human body heat)
    pub hot_pixel_threshold_c: f32,
    /// Minimum connected pixels to count as a person blob
    pub min_blob_pixels: usize,
}

impl Default for ThermalConfig {
    fn default() -> Self {
        Self {
            hot_pixel_threshold_c: 28.0,
            min_blob_pixels: 4,
        }
    }
}

/// CO2 tracker configuration
#[derive(Debug, Clone)]
pub struct Co2Config {
    /// Baseline CO2 level (outdoor/empty room)
    pub baseline_ppm: f32,
    /// Rolling window size in seconds
    pub window_secs: u64,
    /// Approximate CO2 contribution per person
    pub ppm_per_person: f32,
}

impl Default for Co2Config {
    fn default() -> Self {
        Self {
            baseline_ppm: 420.0,
            window_secs: 600, // 10 minutes
            ppm_per_person: 40.0,
        }
    }
}

/// Staleness thresholds for the fusion engine
#[derive(Debug, Clone)]
pub struct FusionConfig {
    pub thermal_stale_secs: u64,
    pub co2_stale_secs: u64,
    pub mmwave_stale_secs: u64,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            thermal_stale_secs: 30,
            co2_stale_secs: 120,
            mmwave_stale_secs: 10,
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

    #[test]
    fn test_sensor_type_as_str() {
        assert_eq!(SensorType::MmWave.as_str(), "mmwave");
        assert_eq!(SensorType::Thermal.as_str(), "thermal");
        assert_eq!(SensorType::Co2.as_str(), "co2");
    }

    #[test]
    fn test_suite_config_from_sensor_config() {
        let legacy = SensorConfig {
            port: "/dev/cu.usbserial-2110".to_string(),
            url: "http://172.16.100.37".to_string(),
            debounce_secs: 15,
            absence_threshold_secs: 180,
            csv_log_enabled: true,
        };
        let suite: SuiteConfig = legacy.into();
        assert_eq!(suite.url, "http://172.16.100.37");
        assert_eq!(suite.debounce_secs, 15);
        assert_eq!(suite.thermal.hot_pixel_threshold_c, 28.0);
        assert_eq!(suite.co2.baseline_ppm, 420.0);
    }

    #[test]
    fn test_occupancy_estimate_default() {
        let est = OccupancyEstimate::default();
        assert_eq!(est.count, None);
        assert_eq!(est.confidence, 0.0);
        assert!(est.contributing_sensors.is_empty());
    }
}
