//! Sensor fusion engine — combines readings from multiple sensors.
//!
//! Currently: mmWave-only passthrough. Thermal and CO2 readings are tracked
//! for health/staleness reporting but do NOT influence the presence decision.
//!
//! Future: per-room calibration profiles will enable sensor fusion. Different
//! rooms have different ventilation (CO2 reliability), sensor placement
//! (mmWave wall vs desk), and hardware (thermal camera may not be present).
//! Until calibration is implemented, mmWave alone is the safest default.
//!
//! All functions are pure — given inputs, expect outputs. No mocking needed.

use std::time::{Duration, Instant};

use super::co2::Co2Trend;
use super::thermal;
use super::types::{
    FusedState, FusionConfig, OccupancyEstimate, PresenceState, SensorHealth, SensorType,
    ThermalConfig,
};

/// Snapshot of all sensor data for a single fusion cycle
pub struct FusionInput {
    /// Latest debounced mmWave state (None if no mmWave sensor)
    pub mmwave_present: Option<bool>,
    pub mmwave_last_reading: Option<Instant>,

    /// Latest thermal frame (None if no thermal sensor)
    pub thermal_frame: Option<ThermalSnapshot>,

    /// CO2 tracker state
    pub co2_elevated: bool,
    pub co2_trend: Co2Trend,
    pub co2_occupancy: Option<u8>,
    pub co2_last_reading: Option<Instant>,

    /// Current time for staleness checks
    pub now: Instant,
}

/// Snapshot of thermal analysis results
#[derive(Clone, Copy)]
pub struct ThermalSnapshot {
    pub is_present: bool,
    pub occupancy_count: u8,
    pub last_reading: Instant,
}

/// Run the fusion algorithm.
///
/// Currently mmWave-only: returns the debounced mmWave state directly.
/// Thermal and CO2 data are recorded in sensor_health for monitoring
/// but do not affect the presence decision.
///
/// Future: per-room calibration will enable multi-sensor fusion.
pub fn fuse(input: &FusionInput, config: &FusionConfig) -> FusedState {
    let thermal_stale = Duration::from_secs(config.thermal_stale_secs);
    let co2_stale = Duration::from_secs(config.co2_stale_secs);
    let mmwave_stale = Duration::from_secs(config.mmwave_stale_secs);

    // Classify each sensor's freshness
    let thermal_fresh = input.thermal_frame.as_ref().is_some_and(|t| {
        input.now.duration_since(t.last_reading) < thermal_stale
    });

    let mmwave_fresh = input.mmwave_last_reading.is_some_and(|ts| {
        input.now.duration_since(ts) < mmwave_stale
    });

    let co2_fresh = input.co2_last_reading.is_some_and(|ts| {
        input.now.duration_since(ts) < co2_stale
    });

    // Build sensor health report (all sensors, regardless of fusion strategy)
    let sensor_health = build_health(input, thermal_fresh, mmwave_fresh, co2_fresh);

    // --- mmWave-only passthrough ---
    if mmwave_fresh {
        let present = input.mmwave_present.unwrap_or(false);
        let presence = if present {
            PresenceState::Present
        } else {
            PresenceState::Absent
        };

        return FusedState {
            presence,
            occupancy: OccupancyEstimate {
                count: if present { Some(1) } else { Some(0) },
                confidence: 1.0,
                contributing_sensors: vec![SensorType::MmWave],
            },
            sensor_health,
        };
    }

    // No fresh mmWave → Unknown
    FusedState {
        presence: PresenceState::Unknown,
        occupancy: OccupancyEstimate::default(),
        sensor_health,
    }
}

/// Convenience function: analyze a thermal frame and return a snapshot.
pub fn analyze_thermal(
    frame: &[f32],
    w: u16,
    h: u16,
    config: &ThermalConfig,
    now: Instant,
) -> ThermalSnapshot {
    ThermalSnapshot {
        is_present: thermal::thermal_presence(frame, config),
        occupancy_count: thermal::estimate_occupancy(frame, w, h, config),
        last_reading: now,
    }
}

fn build_health(
    input: &FusionInput,
    thermal_fresh: bool,
    mmwave_fresh: bool,
    co2_fresh: bool,
) -> Vec<SensorHealth> {
    let mut health = Vec::new();

    if let Some(ts) = input.mmwave_last_reading {
        health.push(SensorHealth {
            sensor_type: SensorType::MmWave,
            last_reading: Some(ts),
            is_stale: !mmwave_fresh,
        });
    }

    if let Some(ref thermal) = input.thermal_frame {
        health.push(SensorHealth {
            sensor_type: SensorType::Thermal,
            last_reading: Some(thermal.last_reading),
            is_stale: !thermal_fresh,
        });
    }

    if let Some(ts) = input.co2_last_reading {
        health.push(SensorHealth {
            sensor_type: SensorType::Co2,
            last_reading: Some(ts),
            is_stale: !co2_fresh,
        });
    }

    health
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> FusionConfig {
        FusionConfig::default()
    }

    fn now() -> Instant {
        Instant::now()
    }

    // ===== No sensors =====

    #[test]
    fn test_all_stale_returns_unknown() {
        let input = FusionInput {
            mmwave_present: None,
            mmwave_last_reading: None,
            thermal_frame: None,
            co2_elevated: false,
            co2_trend: Co2Trend::Stable,
            co2_occupancy: None,
            co2_last_reading: None,
            now: now(),
        };
        let result = fuse(&input, &default_config());
        assert_eq!(result.presence, PresenceState::Unknown);
        assert_eq!(result.occupancy.count, None);
    }

    // ===== mmWave passthrough =====

    #[test]
    fn test_mmwave_present() {
        let n = now();
        let input = FusionInput {
            mmwave_present: Some(true),
            mmwave_last_reading: Some(n),
            thermal_frame: None,
            co2_elevated: false,
            co2_trend: Co2Trend::Stable,
            co2_occupancy: None,
            co2_last_reading: None,
            now: n,
        };
        let result = fuse(&input, &default_config());
        assert_eq!(result.presence, PresenceState::Present);
        assert_eq!(result.occupancy.count, Some(1));
        assert_eq!(result.occupancy.confidence, 1.0);
        assert_eq!(result.occupancy.contributing_sensors, vec![SensorType::MmWave]);
    }

    #[test]
    fn test_mmwave_absent() {
        let n = now();
        let input = FusionInput {
            mmwave_present: Some(false),
            mmwave_last_reading: Some(n),
            thermal_frame: None,
            co2_elevated: false,
            co2_trend: Co2Trend::Stable,
            co2_occupancy: None,
            co2_last_reading: None,
            now: n,
        };
        let result = fuse(&input, &default_config());
        assert_eq!(result.presence, PresenceState::Absent);
        assert_eq!(result.occupancy.count, Some(0));
    }

    #[test]
    fn test_mmwave_ignores_thermal_and_co2() {
        let n = now();
        // mmWave says absent, thermal says present, CO2 elevated —
        // fusion should still return Absent (mmWave-only passthrough)
        let input = FusionInput {
            mmwave_present: Some(false),
            mmwave_last_reading: Some(n),
            thermal_frame: Some(ThermalSnapshot {
                is_present: true,
                occupancy_count: 2,
                last_reading: n,
            }),
            co2_elevated: true,
            co2_trend: Co2Trend::Rising,
            co2_occupancy: Some(2),
            co2_last_reading: Some(n),
            now: n,
        };
        let result = fuse(&input, &default_config());
        assert_eq!(result.presence, PresenceState::Absent);
        assert_eq!(result.occupancy.contributing_sensors, vec![SensorType::MmWave]);
    }

    #[test]
    fn test_stale_mmwave_returns_unknown() {
        let n = now();
        let old = n - Duration::from_secs(60); // 60s ago, threshold is 10s
        let input = FusionInput {
            mmwave_present: Some(true),
            mmwave_last_reading: Some(old),
            thermal_frame: None,
            co2_elevated: false,
            co2_trend: Co2Trend::Stable,
            co2_occupancy: None,
            co2_last_reading: None,
            now: n,
        };
        let result = fuse(&input, &default_config());
        assert_eq!(result.presence, PresenceState::Unknown);
        let mmwave_health = result.sensor_health.iter()
            .find(|h| h.sensor_type == SensorType::MmWave);
        assert!(mmwave_health.is_some_and(|h| h.is_stale));
    }

    // ===== Sensor health =====

    #[test]
    fn test_health_tracks_all_sensors() {
        let n = now();
        let input = FusionInput {
            mmwave_present: Some(true),
            mmwave_last_reading: Some(n),
            thermal_frame: Some(ThermalSnapshot {
                is_present: true,
                occupancy_count: 1,
                last_reading: n,
            }),
            co2_elevated: true,
            co2_trend: Co2Trend::Stable,
            co2_occupancy: Some(1),
            co2_last_reading: Some(n),
            now: n,
        };
        let result = fuse(&input, &default_config());
        // All 3 sensors reported in health, even though only mmWave drives presence
        assert_eq!(result.sensor_health.len(), 3);
        assert!(result.sensor_health.iter().all(|h| !h.is_stale));
    }

    #[test]
    fn test_health_marks_stale_sensors() {
        let n = now();
        let old = n - Duration::from_secs(300);
        let input = FusionInput {
            mmwave_present: Some(true),
            mmwave_last_reading: Some(n),
            thermal_frame: Some(ThermalSnapshot {
                is_present: false,
                occupancy_count: 0,
                last_reading: old,
            }),
            co2_elevated: false,
            co2_trend: Co2Trend::Stable,
            co2_occupancy: None,
            co2_last_reading: Some(old),
            now: n,
        };
        let result = fuse(&input, &default_config());
        let thermal = result.sensor_health.iter()
            .find(|h| h.sensor_type == SensorType::Thermal).unwrap();
        let co2 = result.sensor_health.iter()
            .find(|h| h.sensor_type == SensorType::Co2).unwrap();
        let mmwave = result.sensor_health.iter()
            .find(|h| h.sensor_type == SensorType::MmWave).unwrap();
        assert!(!mmwave.is_stale);
        assert!(thermal.is_stale);
        assert!(co2.is_stale);
    }
}
