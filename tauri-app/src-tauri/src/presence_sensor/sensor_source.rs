//! Sensor source trait — abstraction for different hardware interfaces.
//!
//! Each source produces raw `SensorReading` values via a shared mpsc channel.
//! Sources do NOT debounce — that's the fusion engine's job.
//! Object-safe trait for dynamic composition from config.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::types::{SensorReading, SensorStatus, SensorType};

/// A physical sensor data source (ESP32 HTTP, serial, mock, etc.)
///
/// Sources produce raw readings via a shared mpsc channel. Multiple sources
/// can feed into the same channel for fusion. Sources also report their
/// connection health via a dedicated status channel.
pub trait SensorSource: Send + Sync {
    /// Human-readable name for logging
    fn name(&self) -> &str;

    /// Which sensor types this source provides
    fn provided_sensors(&self) -> Vec<SensorType>;

    /// Start the source. Spawns internal tasks that send readings to `reading_tx`.
    /// Updates `status_tx` with connection health.
    /// Returns a join handle for the spawned task.
    fn start(
        &self,
        reading_tx: mpsc::Sender<SensorReading>,
        status_tx: Arc<watch::Sender<SensorStatus>>,
        stop: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>, String>;
}
