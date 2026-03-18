//! ESP32 HTTP sensor source.
//!
//! Polls the ESP32 WiFi bridge at `{url}/` every ~1s, parsing the full JSON response
//! to produce readings for all available sensors (mmWave, CO2, thermal summary).
//! Optionally fetches `{url}/thermal` for full 768-pixel thermal frames.

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

/// JSON response from the ESP32 presence sensor bridge.
///
/// The ESP32 serves all sensor data from a single `GET /` endpoint.
/// Fields are optional — the response includes only the sensors physically connected.
#[derive(Debug, serde::Deserialize)]
struct Esp32Response {
    /// mmWave presence (SEN0395)
    present: bool,
    #[serde(default)]
    sensor_stale: bool,
    // CO2/environmental data from SCD41
    #[serde(default)]
    co2_ppm: Option<f32>,
    #[serde(default)]
    temperature_c: Option<f32>,
    #[serde(default)]
    humidity_pct: Option<f32>,
    // Thermal summary from MLX90640 (from main endpoint)
    // Reserved for future use — thermal detection via summary stats
    #[serde(default)]
    #[allow(dead_code)]
    thermal_present: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)]
    thermal_max_c: Option<f32>,
}

/// Full thermal frame response from `GET /thermal`
#[derive(Debug, serde::Deserialize)]
struct ThermalFrameResponse {
    #[serde(default)]
    pixels: Vec<f32>,
    #[serde(default = "default_thermal_width")]
    width: u16,
    #[serde(default = "default_thermal_height")]
    height: u16,
}

fn default_thermal_width() -> u16 {
    32
}
fn default_thermal_height() -> u16 {
    24
}

/// ESP32 HTTP sensor source configuration
pub struct Esp32HttpSource {
    url: String,
}

impl Esp32HttpSource {
    pub fn new(url: String) -> Self {
        Self { url }
    }
}

impl SensorSource for Esp32HttpSource {
    fn name(&self) -> &str {
        "esp32_http"
    }

    fn provided_sensors(&self) -> Vec<SensorType> {
        // ESP32 bridge can provide all three sensors
        vec![SensorType::MmWave, SensorType::Thermal, SensorType::Co2]
    }

    fn start(
        &self,
        reading_tx: mpsc::Sender<SensorReading>,
        status_tx: Arc<watch::Sender<SensorStatus>>,
        stop: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>, String> {
        let url = self.url.clone();

        let handle = tokio::spawn(async move {
            esp32_reader_loop(&url, reading_tx, status_tx, stop).await;
        });

        Ok(handle)
    }
}

/// Main polling loop for the ESP32 HTTP bridge.
///
/// Polls `{url}/` every ~1s for mmWave + CO2 data.
/// Polls `{url}/thermal` every ~5s for full thermal frames.
async fn esp32_reader_loop(
    base_url: &str,
    reading_tx: mpsc::Sender<SensorReading>,
    status_tx: Arc<watch::Sender<SensorStatus>>,
    stop: Arc<AtomicBool>,
) {
    let url = if base_url.ends_with('/') {
        base_url.to_string()
    } else {
        format!("{}/", base_url)
    };
    let thermal_url = format!("{}thermal", url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    let mut was_connected = false;
    let mut poll_count: u64 = 0;
    // Fetch thermal frames every ~5 polls (5 seconds at 1Hz poll rate)
    let thermal_interval = 5u64;

    loop {
        if stop.load(Ordering::Relaxed) {
            info!("ESP32 HTTP source stopping (stop flag set)");
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
                                info!("ESP32 sensor connected via HTTP: {}", base_url);
                                let _ = status_tx.send(SensorStatus::Connected);
                                was_connected = true;
                            }

                            let now = Instant::now();

                            // mmWave presence reading
                            let _ = reading_tx
                                .send(SensorReading {
                                    sensor_type: SensorType::MmWave,
                                    timestamp: now,
                                    value: SensorValue::Presence(data.present),
                                })
                                .await;

                            // CO2 reading (if available)
                            if let (Some(ppm), Some(temp), Some(hum)) =
                                (data.co2_ppm, data.temperature_c, data.humidity_pct)
                            {
                                let _ = reading_tx
                                    .send(SensorReading {
                                        sensor_type: SensorType::Co2,
                                        timestamp: now,
                                        value: SensorValue::Co2 {
                                            ppm,
                                            temperature_c: temp,
                                            humidity_pct: hum,
                                        },
                                    })
                                    .await;
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
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
            Err(e) => {
                if was_connected {
                    warn!("ESP32 connection error: {}", e);
                    let _ = status_tx.send(SensorStatus::Disconnected);
                    was_connected = false;
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        }

        // Fetch full thermal frame periodically
        if was_connected && poll_count % thermal_interval == 0 {
            match client.get(&thermal_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<ThermalFrameResponse>().await {
                        Ok(frame) => {
                            if !frame.pixels.is_empty() {
                                let _ = reading_tx
                                    .send(SensorReading {
                                        sensor_type: SensorType::Thermal,
                                        timestamp: Instant::now(),
                                        value: SensorValue::ThermalFrame {
                                            pixels: frame.pixels,
                                            width: frame.width,
                                            height: frame.height,
                                        },
                                    })
                                    .await;
                            }
                        }
                        Err(e) => {
                            debug!("Thermal frame parse error (non-fatal): {}", e);
                        }
                    }
                }
                Ok(_) | Err(_) => {
                    debug!("Thermal endpoint unavailable (non-fatal)");
                }
            }
        }

        poll_count += 1;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_esp32_response_parsing() {
        let json = r#"{"present":true,"sensor_stale":false,"sensor_age_ms":616,"uptime_s":114,"wifi_rssi":-64,"ip":"172.16.100.37"}"#;
        let resp: Esp32Response = serde_json::from_str(json).expect("Should parse ESP32 response");
        assert!(resp.present);
        assert!(!resp.sensor_stale);
        assert!(resp.co2_ppm.is_none());
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
        let json = r#"{"present":true}"#;
        let resp: Esp32Response =
            serde_json::from_str(json).expect("Should parse minimal");
        assert!(resp.present);
        assert!(!resp.sensor_stale);
    }

    #[test]
    fn test_esp32_response_with_co2() {
        let json = r#"{"present":true,"sensor_stale":false,"co2_ppm":450.0,"temperature_c":23.5,"humidity_pct":45.2}"#;
        let resp: Esp32Response = serde_json::from_str(json).expect("Should parse with CO2");
        assert!(resp.present);
        assert_eq!(resp.co2_ppm, Some(450.0));
        assert_eq!(resp.temperature_c, Some(23.5));
        assert_eq!(resp.humidity_pct, Some(45.2));
    }

    #[test]
    fn test_esp32_response_with_thermal_summary() {
        let json = r#"{"present":true,"sensor_stale":false,"thermal_present":true,"thermal_max_c":32.5}"#;
        let resp: Esp32Response =
            serde_json::from_str(json).expect("Should parse with thermal");
        assert_eq!(resp.thermal_present, Some(true));
        assert_eq!(resp.thermal_max_c, Some(32.5));
    }

    #[test]
    fn test_thermal_frame_parsing() {
        let json = r#"{"pixels":[20.0,21.0,30.0],"width":32,"height":24}"#;
        let frame: ThermalFrameResponse =
            serde_json::from_str(json).expect("Should parse thermal frame");
        assert_eq!(frame.pixels.len(), 3);
        assert_eq!(frame.width, 32);
        assert_eq!(frame.height, 24);
    }

    #[test]
    fn test_thermal_frame_defaults() {
        let json = r#"{"pixels":[20.0]}"#;
        let frame: ThermalFrameResponse =
            serde_json::from_str(json).expect("Should parse with defaults");
        assert_eq!(frame.width, 32);
        assert_eq!(frame.height, 24);
    }

    #[test]
    fn test_source_metadata() {
        let source = Esp32HttpSource::new("http://192.168.1.1".to_string());
        assert_eq!(source.name(), "esp32_http");
        let sensors = source.provided_sensors();
        assert!(sensors.contains(&SensorType::MmWave));
        assert!(sensors.contains(&SensorType::Thermal));
        assert!(sensors.contains(&SensorType::Co2));
    }
}
