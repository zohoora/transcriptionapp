//! Mock sensor source for testing.
//!
//! Produces scripted sensor readings on a configurable timeline,
//! enabling deterministic testing of the fusion engine.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::presence_sensor::sensor_source::SensorSource;
use crate::presence_sensor::types::{
    SensorReading, SensorStatus, SensorType, SensorValue,
};

/// A scripted sensor event for mock testing
pub struct MockEvent {
    /// Delay before emitting this reading
    pub delay: Duration,
    /// The reading to emit
    pub reading: MockReading,
}

/// Types of mock readings
pub enum MockReading {
    MmWave(bool),
    ThermalFrame(Vec<f32>, u16, u16),
    Co2 { ppm: f32, temperature_c: f32, humidity_pct: f32 },
}

/// Mock sensor source that replays a scripted timeline
pub struct MockSource {
    name: String,
    sensors: Vec<SensorType>,
    events: Vec<MockEvent>,
}

impl MockSource {
    pub fn new(name: &str, sensors: Vec<SensorType>, events: Vec<MockEvent>) -> Self {
        Self {
            name: name.to_string(),
            sensors,
            events,
        }
    }

    /// Create a simple mmWave mock that emits a sequence of presence values
    pub fn mmwave_sequence(values: Vec<(Duration, bool)>) -> Self {
        let events = values
            .into_iter()
            .map(|(delay, present)| MockEvent {
                delay,
                reading: MockReading::MmWave(present),
            })
            .collect();

        Self {
            name: "mock_mmwave".to_string(),
            sensors: vec![SensorType::MmWave],
            events,
        }
    }
}

impl SensorSource for MockSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn provided_sensors(&self) -> Vec<SensorType> {
        self.sensors.clone()
    }

    fn start(
        &self,
        reading_tx: mpsc::Sender<SensorReading>,
        status_tx: Arc<watch::Sender<SensorStatus>>,
        stop: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>, String> {
        // Convert events to (delay, sensor_type, value) tuples for the spawned task
        let events: Vec<(Duration, SensorType, SensorValue)> = self
            .events
            .iter()
            .map(|e| match &e.reading {
                MockReading::MmWave(v) => {
                    (e.delay, SensorType::MmWave, SensorValue::Presence(*v))
                }
                MockReading::ThermalFrame(p, w, h) => (
                    e.delay,
                    SensorType::Thermal,
                    SensorValue::ThermalFrame { pixels: p.clone(), width: *w, height: *h },
                ),
                MockReading::Co2 { ppm, temperature_c, humidity_pct } => (
                    e.delay,
                    SensorType::Co2,
                    SensorValue::Co2 {
                        ppm: *ppm,
                        temperature_c: *temperature_c,
                        humidity_pct: *humidity_pct,
                    },
                ),
            })
            .collect();

        let handle = tokio::spawn(async move {
            let _ = status_tx.send(SensorStatus::Connected);

            for (delay, sensor_type, value) in events {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                tokio::time::sleep(delay).await;

                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let _ = reading_tx
                    .send(SensorReading {
                        sensor_type,
                        timestamp: Instant::now(),
                        value,
                    })
                    .await;
            }

            // Keep alive until stopped (don't drop channels)
            while !stop.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });

        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_source_metadata() {
        let source = MockSource::mmwave_sequence(vec![
            (Duration::from_millis(0), true),
            (Duration::from_secs(1), false),
        ]);
        assert_eq!(source.name(), "mock_mmwave");
        assert_eq!(source.provided_sensors(), vec![SensorType::MmWave]);
    }

    #[test]
    fn test_mock_multi_sensor() {
        let source = MockSource::new(
            "mock_all",
            vec![SensorType::MmWave, SensorType::Co2],
            vec![
                MockEvent {
                    delay: Duration::from_millis(0),
                    reading: MockReading::MmWave(true),
                },
                MockEvent {
                    delay: Duration::from_millis(100),
                    reading: MockReading::Co2 {
                        ppm: 450.0,
                        temperature_c: 23.0,
                        humidity_pct: 45.0,
                    },
                },
            ],
        );
        assert_eq!(source.name(), "mock_all");
        assert_eq!(source.provided_sensors().len(), 2);
    }

    #[tokio::test]
    async fn test_mock_source_emits_readings() {
        let source = MockSource::mmwave_sequence(vec![
            (Duration::from_millis(0), true),
            (Duration::from_millis(10), false),
        ]);

        let (reading_tx, mut reading_rx) = mpsc::channel(32);
        let (status_tx, _) = watch::channel(SensorStatus::Disconnected);
        let status_tx = Arc::new(status_tx);
        let stop = Arc::new(AtomicBool::new(false));

        let handle = source.start(reading_tx, status_tx, stop.clone()).unwrap();

        // Should receive two readings
        let r1 = tokio::time::timeout(Duration::from_secs(1), reading_rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");
        assert!(matches!(r1.value, SensorValue::Presence(true)));

        let r2 = tokio::time::timeout(Duration::from_secs(1), reading_rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");
        assert!(matches!(r2.value, SensorValue::Presence(false)));

        stop.store(true, Ordering::Relaxed);
        handle.abort();
    }
}
