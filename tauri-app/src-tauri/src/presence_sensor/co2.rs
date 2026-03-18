//! CO2 rolling tracker with trend analysis and occupancy estimation.
//!
//! CO2 level correlates with occupancy count (~40 ppm per person above baseline)
//! with a ~6 minute lag. Best used for validation, not real-time detection.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::types::Co2Config;

/// CO2 trend direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Co2Trend {
    Rising,
    Falling,
    Stable,
}

/// Rolling CO2 tracker with trend analysis.
///
/// Maintains a sliding window of readings and computes:
/// - Whether CO2 is elevated above baseline (presence indicator)
/// - Occupancy estimate based on ppm delta from baseline
/// - Trend direction (rising/falling/stable)
pub struct Co2Tracker {
    readings: VecDeque<(Instant, f32)>,
    config: Co2Config,
}

impl Co2Tracker {
    pub fn new(config: Co2Config) -> Self {
        Self {
            readings: VecDeque::new(),
            config,
        }
    }

    /// Add a new CO2 reading, pruning old entries outside the window.
    pub fn add_reading(&mut self, ppm: f32, now: Instant) {
        let window = Duration::from_secs(self.config.window_secs);

        // Prune old readings
        while let Some(&(ts, _)) = self.readings.front() {
            if now.duration_since(ts) > window {
                self.readings.pop_front();
            } else {
                break;
            }
        }

        self.readings.push_back((now, ppm));
    }

    /// Whether CO2 is elevated above baseline (indicates room is occupied).
    ///
    /// Uses half of `ppm_per_person` as the threshold — even a fractional
    /// person's worth of CO2 elevation suggests someone is present.
    pub fn is_elevated(&self) -> bool {
        let avg = self.average_ppm();
        avg > self.config.baseline_ppm + (self.config.ppm_per_person / 2.0)
    }

    /// Estimate occupancy based on CO2 elevation above baseline.
    ///
    /// Returns None if not enough data (fewer than 3 readings).
    /// Each ~40 ppm above baseline ≈ 1 person.
    pub fn estimated_occupancy(&self) -> Option<u8> {
        if self.readings.len() < 3 {
            return None;
        }

        let avg = self.average_ppm();
        let delta = avg - self.config.baseline_ppm;

        if delta <= 0.0 {
            return Some(0);
        }

        let estimate = (delta / self.config.ppm_per_person).round() as u8;
        Some(estimate)
    }

    /// Current trend direction based on linear regression over the window.
    ///
    /// Rising: slope > +0.5 ppm/min
    /// Falling: slope < -0.5 ppm/min
    /// Stable: otherwise
    pub fn trend(&self) -> Co2Trend {
        if self.readings.len() < 5 {
            return Co2Trend::Stable;
        }

        let slope = self.compute_slope();

        // Convert to ppm/min for human-readable thresholds
        let slope_per_min = slope * 60.0;

        if slope_per_min > 0.5 {
            Co2Trend::Rising
        } else if slope_per_min < -0.5 {
            Co2Trend::Falling
        } else {
            Co2Trend::Stable
        }
    }

    /// Get the latest CO2 reading value, if any.
    pub fn latest_ppm(&self) -> Option<f32> {
        self.readings.back().map(|&(_, ppm)| ppm)
    }

    /// Number of readings in the window.
    pub fn reading_count(&self) -> usize {
        self.readings.len()
    }

    // --- Private helpers ---

    fn average_ppm(&self) -> f32 {
        if self.readings.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.readings.iter().map(|&(_, ppm)| ppm).sum();
        sum / self.readings.len() as f32
    }

    /// Simple linear regression: slope of ppm over time (ppm/second).
    fn compute_slope(&self) -> f32 {
        let n = self.readings.len() as f32;
        if n < 2.0 {
            return 0.0;
        }

        let first_ts = self.readings.front().unwrap().0;

        let mut sum_x: f64 = 0.0;
        let mut sum_y: f64 = 0.0;
        let mut sum_xy: f64 = 0.0;
        let mut sum_xx: f64 = 0.0;

        for &(ts, ppm) in &self.readings {
            let x = ts.duration_since(first_ts).as_secs_f64();
            let y = ppm as f64;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let n = n as f64;
        let denom = n * sum_xx - sum_x * sum_x;
        if denom.abs() < 1e-10 {
            return 0.0;
        }

        let slope = (n * sum_xy - sum_x * sum_y) / denom;
        slope as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Co2Config {
        Co2Config {
            baseline_ppm: 420.0,
            window_secs: 600,
            ppm_per_person: 40.0,
        }
    }

    #[test]
    fn test_empty_tracker() {
        let tracker = Co2Tracker::new(test_config());
        assert!(!tracker.is_elevated());
        assert_eq!(tracker.estimated_occupancy(), None);
        assert_eq!(tracker.trend(), Co2Trend::Stable);
        assert_eq!(tracker.latest_ppm(), None);
    }

    #[test]
    fn test_single_reading() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        tracker.add_reading(450.0, now);
        assert!(tracker.is_elevated()); // 450 > 420 + 20
        assert_eq!(tracker.latest_ppm(), Some(450.0));
        assert_eq!(tracker.reading_count(), 1);
    }

    #[test]
    fn test_not_elevated_at_baseline() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        tracker.add_reading(420.0, now);
        assert!(!tracker.is_elevated());
    }

    #[test]
    fn test_barely_elevated() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        // 441 > 420 + 20 = 440 (threshold is half of ppm_per_person)
        tracker.add_reading(441.0, now);
        assert!(tracker.is_elevated());
    }

    #[test]
    fn test_barely_not_elevated() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        // 439 < 420 + 20 = 440
        tracker.add_reading(439.0, now);
        assert!(!tracker.is_elevated());
    }

    #[test]
    fn test_occupancy_one_person() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        for i in 0..5 {
            tracker.add_reading(460.0, now + Duration::from_secs(i * 5));
        }
        // 460 - 420 = 40 ppm → ~1 person
        assert_eq!(tracker.estimated_occupancy(), Some(1));
    }

    #[test]
    fn test_occupancy_two_people() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        for i in 0..5 {
            tracker.add_reading(500.0, now + Duration::from_secs(i * 5));
        }
        // 500 - 420 = 80 ppm → ~2 people
        assert_eq!(tracker.estimated_occupancy(), Some(2));
    }

    #[test]
    fn test_occupancy_empty_room() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        for i in 0..5 {
            tracker.add_reading(415.0, now + Duration::from_secs(i * 5));
        }
        // Below baseline → 0
        assert_eq!(tracker.estimated_occupancy(), Some(0));
    }

    #[test]
    fn test_occupancy_insufficient_data() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        tracker.add_reading(500.0, now);
        tracker.add_reading(500.0, now + Duration::from_secs(5));
        // Only 2 readings, need 3
        assert_eq!(tracker.estimated_occupancy(), None);
    }

    #[test]
    fn test_trend_rising() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        // CO2 rising at ~2 ppm/min
        for i in 0..10 {
            let ppm = 420.0 + (i as f32 * 2.0);
            tracker.add_reading(ppm, now + Duration::from_secs(i * 60));
        }
        assert_eq!(tracker.trend(), Co2Trend::Rising);
    }

    #[test]
    fn test_trend_falling() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        // CO2 falling at ~2 ppm/min
        for i in 0..10 {
            let ppm = 500.0 - (i as f32 * 2.0);
            tracker.add_reading(ppm, now + Duration::from_secs(i * 60));
        }
        assert_eq!(tracker.trend(), Co2Trend::Falling);
    }

    #[test]
    fn test_trend_stable() {
        let mut tracker = Co2Tracker::new(test_config());
        let now = Instant::now();
        for i in 0..10 {
            tracker.add_reading(450.0, now + Duration::from_secs(i * 60));
        }
        assert_eq!(tracker.trend(), Co2Trend::Stable);
    }

    #[test]
    fn test_window_pruning() {
        let mut tracker = Co2Tracker::new(Co2Config {
            baseline_ppm: 420.0,
            window_secs: 60, // 1 minute window
            ppm_per_person: 40.0,
        });
        let now = Instant::now();

        // Add readings spanning 2 minutes
        tracker.add_reading(500.0, now);
        tracker.add_reading(500.0, now + Duration::from_secs(30));
        tracker.add_reading(420.0, now + Duration::from_secs(90));
        tracker.add_reading(420.0, now + Duration::from_secs(120));

        // Only readings within the last 60s from the latest (t=120) should remain
        // That's t=90 and t=120
        assert_eq!(tracker.reading_count(), 2);
    }
}
