//! CO2 sensor calibration tool.
//!
//! Guides the user through a multi-phase calibration to determine per-room
//! CO2 baseline, per-person contribution, and response characteristics.
//! Runs as a tokio task, communicates with the frontend via Tauri events.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::Emitter;
use tokio::sync::mpsc;
use tracing::{info, warn};

// --- Types ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalibrationPhase {
    Idle,
    Connecting,
    EmptyRoom,
    OnePerson,
    TwoPeople,
    ThreePlus,
    Computing,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseReading {
    pub timestamp_secs: f64,
    pub co2_ppm: f32,
    pub temperature_c: f32,
    pub humidity_pct: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseResult {
    pub phase: CalibrationPhase,
    pub occupancy: u8,
    pub stable_avg_ppm: f32,
    pub stable_std_dev: f32,
    pub readings_collected: usize,
    pub duration_secs: f64,
    pub was_skipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationResult {
    pub baseline_ppm: f32,
    pub ppm_per_person: f32,
    pub recommended_window_secs: u64,
    pub rise_rate_ppm_per_min: f32,
    pub fall_rate_ppm_per_min: f32,
    pub phase_results: Vec<PhaseResult>,
    pub calibrated_at: String,
    pub room_id: String,
    pub sensor_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationStatus {
    pub phase: CalibrationPhase,
    pub current_co2_ppm: Option<f32>,
    pub current_temp_c: Option<f32>,
    pub current_humidity_pct: Option<f32>,
    pub mmwave_present: Option<bool>,
    pub phase_elapsed_secs: f64,
    pub is_stable: bool,
    pub rate_of_change_ppm_per_min: f32,
    pub readings_in_phase: usize,
    pub sensor_connected: bool,
    pub result: Option<CalibrationResult>,
    pub error: Option<String>,
    /// Recent CO2 readings for sparkline (last 120 values)
    pub sparkline: Vec<f32>,
}

#[derive(Debug)]
pub enum CalibrationCommand {
    Advance { skip: bool },
    Stop,
}

// ESP32 response (same as esp32_http.rs)
#[derive(Debug, Deserialize)]
struct Esp32Response {
    present: bool,
    #[serde(default)]
    co2_ppm: Option<f32>,
    #[serde(default)]
    temperature_c: Option<f32>,
    #[serde(default)]
    humidity_pct: Option<f32>,
}

// --- Handle ---

pub struct CalibrationHandle {
    pub stop_flag: Arc<AtomicBool>,
    pub command_tx: mpsc::Sender<CalibrationCommand>,
}

impl CalibrationHandle {
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

// --- Stability detection ---

const STABILITY_WINDOW_SECS: f64 = 60.0;
const STABILITY_THRESHOLD_PPM_PER_MIN: f32 = 0.3;
const MIN_READINGS_FOR_STABILITY: usize = 30;
const MIN_EMPTY_ROOM_SECS: f64 = 180.0;
const MIN_OCCUPIED_SECS: f64 = 120.0;

fn compute_slope(readings: &[(f64, f32)]) -> f32 {
    let n = readings.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mut sum_x: f64 = 0.0;
    let mut sum_y: f64 = 0.0;
    let mut sum_xy: f64 = 0.0;
    let mut sum_xx: f64 = 0.0;
    let first_x = readings[0].0;
    for &(x, y) in readings {
        let x = x - first_x;
        let y = y as f64;
        sum_x += x;
        sum_y += y;
        sum_xy += x * y;
        sum_xx += x * x;
    }
    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-10 {
        return 0.0;
    }
    ((n * sum_xy - sum_x * sum_y) / denom) as f32
}

/// Returns (is_stable, rate_of_change_ppm_per_min) from a single trailing-window pass.
fn stability_metrics(readings: &[PhaseReading], phase: CalibrationPhase) -> (bool, f32) {
    if readings.len() < 5 {
        return (false, 0.0);
    }
    let elapsed = readings.last().unwrap().timestamp_secs - readings.first().unwrap().timestamp_secs;
    let min_secs = if phase == CalibrationPhase::EmptyRoom {
        MIN_EMPTY_ROOM_SECS
    } else {
        MIN_OCCUPIED_SECS
    };

    let cutoff = readings.last().unwrap().timestamp_secs - STABILITY_WINDOW_SECS;
    let trailing: Vec<(f64, f32)> = readings
        .iter()
        .filter(|r| r.timestamp_secs >= cutoff)
        .map(|r| (r.timestamp_secs, r.co2_ppm))
        .collect();

    if trailing.len() < MIN_READINGS_FOR_STABILITY {
        return (false, 0.0);
    }

    let slope_per_min = compute_slope(&trailing) * 60.0;
    let stable = elapsed >= min_secs && slope_per_min.abs() < STABILITY_THRESHOLD_PPM_PER_MIN;
    (stable, slope_per_min)
}

fn stable_average(readings: &[PhaseReading]) -> (f32, f32) {
    if readings.is_empty() {
        return (0.0, 0.0);
    }
    let cutoff = readings.last().unwrap().timestamp_secs - STABILITY_WINDOW_SECS;
    let trailing: Vec<f32> = readings
        .iter()
        .filter(|r| r.timestamp_secs >= cutoff)
        .map(|r| r.co2_ppm)
        .collect();
    if trailing.is_empty() {
        return (0.0, 0.0);
    }
    let avg = trailing.iter().sum::<f32>() / trailing.len() as f32;
    let variance = trailing.iter().map(|v| (v - avg).powi(2)).sum::<f32>() / trailing.len() as f32;
    (avg, variance.sqrt())
}

fn phase_occupancy(phase: CalibrationPhase) -> u8 {
    match phase {
        CalibrationPhase::EmptyRoom => 0,
        CalibrationPhase::OnePerson => 1,
        CalibrationPhase::TwoPeople => 2,
        CalibrationPhase::ThreePlus => 3,
        _ => 0,
    }
}

fn next_phase(current: CalibrationPhase) -> CalibrationPhase {
    match current {
        CalibrationPhase::Connecting => CalibrationPhase::EmptyRoom,
        CalibrationPhase::EmptyRoom => CalibrationPhase::OnePerson,
        CalibrationPhase::OnePerson => CalibrationPhase::TwoPeople,
        CalibrationPhase::TwoPeople => CalibrationPhase::ThreePlus,
        CalibrationPhase::ThreePlus => CalibrationPhase::Computing,
        _ => CalibrationPhase::Complete,
    }
}

// --- Result computation ---

fn compute_results(
    phase_results: &[PhaseResult],
    room_id: &str,
    sensor_url: &str,
    all_readings: &[PhaseReading],
) -> CalibrationResult {
    // Baseline from empty room phase (or fallback)
    let baseline_ppm = phase_results
        .iter()
        .find(|p| p.phase == CalibrationPhase::EmptyRoom && !p.was_skipped)
        .map(|p| p.stable_avg_ppm)
        .unwrap_or_else(|| {
            // Fallback: lowest phase average minus 20
            phase_results
                .iter()
                .filter(|p| !p.was_skipped)
                .map(|p| p.stable_avg_ppm)
                .fold(f32::MAX, f32::min)
                - 20.0
        })
        .max(300.0); // floor at 300 ppm

    // PPM per person: linear fit across non-skipped occupied phases
    let occupied: Vec<(f32, f32)> = phase_results
        .iter()
        .filter(|p| !p.was_skipped && p.occupancy > 0)
        .map(|p| (p.occupancy as f32, p.stable_avg_ppm - baseline_ppm))
        .collect();

    let ppm_per_person = if occupied.len() >= 2 {
        // Linear fit: delta_ppm = slope * occupancy
        let n = occupied.len() as f32;
        let sum_x: f32 = occupied.iter().map(|(x, _)| x).sum();
        let sum_y: f32 = occupied.iter().map(|(_, y)| y).sum();
        let sum_xy: f32 = occupied.iter().map(|(x, y)| x * y).sum();
        let sum_xx: f32 = occupied.iter().map(|(x, _)| x * x).sum();
        let denom = n * sum_xx - sum_x * sum_x;
        if denom.abs() > 1e-6 {
            ((n * sum_xy - sum_x * sum_y) / denom).max(10.0)
        } else {
            40.0
        }
    } else if let Some((occ, delta)) = occupied.first() {
        // Single phase: simple division
        (delta / occ).max(10.0)
    } else {
        40.0 // default
    };

    // Rise/fall rates from all readings
    let rise_rate = compute_transition_rate(all_readings, true);
    let fall_rate = compute_transition_rate(all_readings, false);

    // Recommended window: estimate time to stabilize from occupied phases
    let time_to_stabilize = phase_results
        .iter()
        .filter(|p| !p.was_skipped && p.occupancy > 0)
        .map(|p| p.duration_secs)
        .fold(0.0f64, f64::max);
    let recommended_window = ((time_to_stabilize * 1.5) as u64).clamp(120, 900);

    CalibrationResult {
        baseline_ppm,
        ppm_per_person,
        recommended_window_secs: recommended_window,
        rise_rate_ppm_per_min: rise_rate,
        fall_rate_ppm_per_min: fall_rate,
        phase_results: phase_results.to_vec(),
        calibrated_at: chrono::Utc::now().to_rfc3339(),
        room_id: room_id.to_string(),
        sensor_url: sensor_url.to_string(),
    }
}

fn compute_transition_rate(readings: &[PhaseReading], rising: bool) -> f32 {
    if readings.len() < 60 {
        return 0.0;
    }
    let mut max_rate: f32 = 0.0;
    let mut buf: Vec<(f64, f32)> = Vec::with_capacity(60);
    for window_start in 0..readings.len().saturating_sub(60) {
        buf.clear();
        buf.extend(
            readings[window_start..window_start + 60]
                .iter()
                .map(|r| (r.timestamp_secs, r.co2_ppm)),
        );
        let slope_per_min = compute_slope(&buf) * 60.0;
        if rising && slope_per_min > max_rate {
            max_rate = slope_per_min;
        } else if !rising && slope_per_min < -max_rate.abs() {
            max_rate = slope_per_min.abs();
        }
    }
    max_rate
}

fn build_status(
    phase: CalibrationPhase,
    is_stable: bool,
    rate_of_change: f32,
    last_response: &Option<Esp32Response>,
    phase_readings: &[PhaseReading],
    consecutive_failures: u32,
    phase_start: &Instant,
    result: &Option<CalibrationResult>,
    sparkline: &VecDeque<f32>,
    error: Option<String>,
) -> CalibrationStatus {
    CalibrationStatus {
        phase,
        current_co2_ppm: last_response.as_ref().and_then(|r| r.co2_ppm),
        current_temp_c: last_response.as_ref().and_then(|r| r.temperature_c),
        current_humidity_pct: last_response.as_ref().and_then(|r| r.humidity_pct),
        mmwave_present: last_response.as_ref().map(|r| r.present),
        phase_elapsed_secs: phase_start.elapsed().as_secs_f64(),
        is_stable,
        rate_of_change_ppm_per_min: rate_of_change,
        readings_in_phase: phase_readings.len(),
        sensor_connected: consecutive_failures < 5,
        result: result.clone(),
        error,
        sparkline: sparkline.iter().copied().collect(),
    }
}

pub async fn run_calibration(
    sensor_url: String,
    room_id: String,
    app: tauri::AppHandle,
    stop_flag: Arc<AtomicBool>,
    mut command_rx: mpsc::Receiver<CalibrationCommand>,
) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();

    let mut phase = CalibrationPhase::Connecting;
    let mut phase_readings: Vec<PhaseReading> = Vec::new();
    let mut all_readings: Vec<PhaseReading> = Vec::new();
    let mut phase_results: Vec<PhaseResult> = Vec::new();
    let mut sparkline: VecDeque<f32> = VecDeque::new();
    let calibration_start = Instant::now();
    let mut phase_start = Instant::now();
    let mut consecutive_failures: u32 = 0;
    let mut last_response: Option<Esp32Response> = None;
    let mut result: Option<CalibrationResult> = None;

    info!("CO2 calibration started for room {} (sensor: {})", room_id, sensor_url);

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            info!("CO2 calibration stopped by user");
            break;
        }

        // Check for commands (non-blocking)
        while let Ok(cmd) = command_rx.try_recv() {
            match cmd {
                CalibrationCommand::Advance { skip } => {
                    if matches!(phase, CalibrationPhase::EmptyRoom | CalibrationPhase::OnePerson | CalibrationPhase::TwoPeople | CalibrationPhase::ThreePlus) {
                        let (avg, std_dev) = stable_average(&phase_readings);
                        phase_results.push(PhaseResult {
                            phase,
                            occupancy: phase_occupancy(phase),
                            stable_avg_ppm: avg,
                            stable_std_dev: std_dev,
                            readings_collected: phase_readings.len(),
                            duration_secs: phase_start.elapsed().as_secs_f64(),
                            was_skipped: skip,
                        });
                        info!(
                            "Phase {:?} {} — avg={:.1} ppm, std_dev={:.1}, {} readings",
                            phase,
                            if skip { "skipped" } else { "completed" },
                            avg, std_dev, phase_readings.len()
                        );
                        phase = next_phase(phase);
                        phase_readings.clear();
                        phase_start = Instant::now();
                    }
                }
                CalibrationCommand::Stop => {
                    stop_flag.store(true, Ordering::Relaxed);
                }
            }
        }

        // Computing phase
        if phase == CalibrationPhase::Computing {
            result = Some(compute_results(&phase_results, &room_id, &sensor_url, &all_readings));
            phase = CalibrationPhase::Complete;
            info!("Calibration complete: {:?}", result);
        }

        // Complete or Failed — emit and wait
        if matches!(phase, CalibrationPhase::Complete | CalibrationPhase::Failed) {
            let status = build_status(
                phase, false, 0.0, &last_response, &phase_readings, consecutive_failures,
                &phase_start, &result, &sparkline,
                if phase == CalibrationPhase::Failed { Some("Sensor connection lost".to_string()) } else { None },
            );
            let _ = app.emit("calibration_update", &status);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            continue;
        }

        // Poll sensor
        let url = format!("{}/", sensor_url.trim_end_matches('/'));
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(data) = resp.json::<Esp32Response>().await {
                    consecutive_failures = 0;

                    if let (Some(co2), Some(temp), Some(humidity)) =
                        (data.co2_ppm, data.temperature_c, data.humidity_pct)
                    {
                        sparkline.push_back(co2);
                        if sparkline.len() > 120 {
                            sparkline.pop_front();
                        }

                        let reading = PhaseReading {
                            timestamp_secs: calibration_start.elapsed().as_secs_f64(),
                            co2_ppm: co2,
                            temperature_c: temp,
                            humidity_pct: humidity,
                        };

                        if phase == CalibrationPhase::Connecting {
                            phase = CalibrationPhase::EmptyRoom;
                            phase_start = Instant::now();
                            info!("Sensor connected, starting Empty Room phase");
                        }

                        if matches!(phase, CalibrationPhase::EmptyRoom | CalibrationPhase::OnePerson | CalibrationPhase::TwoPeople | CalibrationPhase::ThreePlus) {
                            phase_readings.push(reading.clone());
                        }
                        all_readings.push(reading);
                    }
                    last_response = Some(data);
                }
            }
            _ => {
                consecutive_failures += 1;
                if consecutive_failures >= 15 && phase != CalibrationPhase::Connecting {
                    warn!("CO2 calibration: 15 consecutive sensor failures, aborting");
                    phase = CalibrationPhase::Failed;
                }
            }
        }

        // Emit status (single stability computation)
        let (stable, roc) = stability_metrics(&phase_readings, phase);
        let error = if consecutive_failures >= 5 {
            Some(format!("Sensor unreachable ({} failures)", consecutive_failures))
        } else {
            None
        };
        let status = build_status(
            phase, stable, roc, &last_response, &phase_readings, consecutive_failures,
            &phase_start, &None, &sparkline, error,
        );
        let _ = app.emit("calibration_update", &status);

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    info!("CO2 calibration loop ended");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_slope_flat() {
        let readings: Vec<(f64, f32)> = (0..60).map(|i| (i as f64, 420.0)).collect();
        let slope = compute_slope(&readings);
        assert!(slope.abs() < 0.01, "flat readings should have ~0 slope: {}", slope);
    }

    #[test]
    fn test_compute_slope_rising() {
        // 1 ppm per second = 60 ppm/min
        let readings: Vec<(f64, f32)> = (0..60).map(|i| (i as f64, 420.0 + i as f32)).collect();
        let slope = compute_slope(&readings);
        assert!((slope - 1.0).abs() < 0.01, "slope should be ~1.0 ppm/s: {}", slope);
    }

    #[test]
    fn test_stability_too_few_readings() {
        let readings: Vec<PhaseReading> = (0..10)
            .map(|i| PhaseReading {
                timestamp_secs: i as f64,
                co2_ppm: 420.0,
                temperature_c: 22.0,
                humidity_pct: 45.0,
            })
            .collect();
        assert!(!stability_metrics(&readings, CalibrationPhase::OnePerson).0);
    }

    #[test]
    fn test_stability_flat_readings() {
        let readings: Vec<PhaseReading> = (0..=180)
            .map(|i| PhaseReading {
                timestamp_secs: i as f64,
                co2_ppm: 420.0 + (i as f32 * 0.001), // tiny drift
                temperature_c: 22.0,
                humidity_pct: 45.0,
            })
            .collect();
        assert!(stability_metrics(&readings, CalibrationPhase::EmptyRoom).0);
        assert!(stability_metrics(&readings, CalibrationPhase::OnePerson).0);
    }

    #[test]
    fn test_compute_results_basic() {
        let phase_results = vec![
            PhaseResult {
                phase: CalibrationPhase::EmptyRoom,
                occupancy: 0,
                stable_avg_ppm: 425.0,
                stable_std_dev: 2.0,
                readings_collected: 180,
                duration_secs: 180.0,
                was_skipped: false,
            },
            PhaseResult {
                phase: CalibrationPhase::OnePerson,
                occupancy: 1,
                stable_avg_ppm: 465.0,
                stable_std_dev: 3.0,
                readings_collected: 120,
                duration_secs: 120.0,
                was_skipped: false,
            },
            PhaseResult {
                phase: CalibrationPhase::TwoPeople,
                occupancy: 2,
                stable_avg_ppm: 505.0,
                stable_std_dev: 4.0,
                readings_collected: 120,
                duration_secs: 120.0,
                was_skipped: false,
            },
        ];
        let result = compute_results(&phase_results, "room-1", "http://sensor", &[]);
        assert!((result.baseline_ppm - 425.0).abs() < 0.1);
        assert!((result.ppm_per_person - 40.0).abs() < 1.0, "ppm_per_person: {}", result.ppm_per_person);
    }

    #[test]
    fn test_phase_occupancy() {
        assert_eq!(phase_occupancy(CalibrationPhase::EmptyRoom), 0);
        assert_eq!(phase_occupancy(CalibrationPhase::OnePerson), 1);
        assert_eq!(phase_occupancy(CalibrationPhase::TwoPeople), 2);
        assert_eq!(phase_occupancy(CalibrationPhase::ThreePlus), 3);
    }

    #[test]
    fn test_next_phase() {
        assert_eq!(next_phase(CalibrationPhase::Connecting), CalibrationPhase::EmptyRoom);
        assert_eq!(next_phase(CalibrationPhase::EmptyRoom), CalibrationPhase::OnePerson);
        assert_eq!(next_phase(CalibrationPhase::ThreePlus), CalibrationPhase::Computing);
    }
}
