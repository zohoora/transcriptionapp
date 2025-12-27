use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use ringbuf::traits::Producer as ProducerTrait;
use ringbuf::HeapProd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Audio device information
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// List available input devices
pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let default_device = host.default_input_device();
    let default_name = default_device
        .as_ref()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();

    for device in host.input_devices().context("Failed to enumerate input devices")? {
        if let Ok(name) = device.name() {
            let is_default = name == default_name;
            devices.push(AudioDevice {
                id: name.clone(),
                name: name.clone(),
                is_default,
            });
        }
    }

    Ok(devices)
}

/// Get device by ID (name) or return default
pub fn get_device(device_id: Option<&str>) -> Result<Device> {
    let host = cpal::default_host();

    match device_id {
        Some(id) if id != "default" => {
            for device in host.input_devices().context("Failed to enumerate devices")? {
                if let Ok(name) = device.name() {
                    if name == id {
                        return Ok(device);
                    }
                }
            }
            anyhow::bail!("Device not found: {}", id);
        }
        _ => host
            .default_input_device()
            .context("No default input device available"),
    }
}

/// Selected audio configuration with both stream config and sample format
pub struct SelectedConfig {
    pub config: StreamConfig,
    pub sample_format: SampleFormat,
}

/// Select the best input configuration for a device
pub fn select_input_config(device: &Device) -> Result<SelectedConfig> {
    // First try to find a mono config
    if let Ok(supported) = device.supported_input_configs() {
        for config_range in supported {
            if config_range.channels() == 1 {
                let supported_config = config_range.with_max_sample_rate();
                debug!(
                    "Selected mono config: {} Hz, {} channels, format {:?}",
                    supported_config.sample_rate().0,
                    supported_config.channels(),
                    supported_config.sample_format()
                );
                return Ok(SelectedConfig {
                    config: supported_config.clone().into(),
                    sample_format: supported_config.sample_format(),
                });
            }
        }
    }

    // Fall back to default (will downmix in callback)
    let supported_config = device
        .default_input_config()
        .context("No default input config")?;
    debug!(
        "Using default config (will downmix): {} Hz, {} channels, format {:?}",
        supported_config.sample_rate().0,
        supported_config.channels(),
        supported_config.sample_format()
    );
    Ok(SelectedConfig {
        config: supported_config.clone().into(),
        sample_format: supported_config.sample_format(),
    })
}

/// Calculate ring buffer capacity for given sample rate
pub fn calculate_ring_buffer_capacity(device_sample_rate: u32) -> usize {
    const BUFFER_DURATION_SECONDS: u32 = 30;
    (device_sample_rate * BUFFER_DURATION_SECONDS) as usize
}

/// Audio capture handle
pub struct AudioCapture {
    stream: Stream,
    sample_rate: u32,
    channels: u16,
    overflow_counter: Arc<AtomicU64>,
    is_running: Arc<AtomicBool>,
}

impl AudioCapture {
    /// Build an input stream that writes to the given ring buffer producer
    pub fn new(
        device: &Device,
        config: &StreamConfig,
        sample_format: SampleFormat,
        mut producer: HeapProd<f32>,
    ) -> Result<Self> {
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;
        let overflow_counter = Arc::new(AtomicU64::new(0));
        let overflow_clone = overflow_counter.clone();
        let is_running = Arc::new(AtomicBool::new(false));
        let running_clone = is_running.clone();

        info!(
            "Building input stream: {} Hz, {} channels, format {:?}",
            sample_rate, channels, sample_format
        );

        let error_callback = |err| {
            error!("Audio stream error: {}", err);
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _| {
                    if !running_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    Self::handle_input_f32(data, channels, &mut producer, &overflow_clone);
                },
                error_callback,
                None,
            ),
            SampleFormat::I16 => {
                let running = is_running.clone();
                device.build_input_stream(
                    config,
                    move |data: &[i16], _| {
                        if !running.load(Ordering::Relaxed) {
                            return;
                        }
                        Self::handle_input_i16(data, channels, &mut producer, &overflow_clone);
                    },
                    error_callback,
                    None,
                )
            }
            SampleFormat::U8 => {
                let running = is_running.clone();
                device.build_input_stream(
                    config,
                    move |data: &[u8], _| {
                        if !running.load(Ordering::Relaxed) {
                            return;
                        }
                        Self::handle_input_u8(data, channels, &mut producer, &overflow_clone);
                    },
                    error_callback,
                    None,
                )
            }
            _ => anyhow::bail!("Unsupported sample format: {:?}", sample_format),
        }
        .context("Failed to build input stream")?;

        Ok(Self {
            stream,
            sample_rate,
            channels: config.channels,
            overflow_counter,
            is_running,
        })
    }

    /// Handle f32 input samples
    fn handle_input_f32(
        data: &[f32],
        channels: usize,
        producer: &mut HeapProd<f32>,
        overflow_counter: &AtomicU64,
    ) {
        if channels == 1 {
            // Mono: push directly
            let pushed = producer.push_slice(data);
            if pushed < data.len() {
                overflow_counter.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            // Downmix: take first channel only
            for chunk in data.chunks(channels) {
                if producer.try_push(chunk[0]).is_err() {
                    overflow_counter.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
    }

    /// Handle i16 input samples (convert to f32)
    fn handle_input_i16(
        data: &[i16],
        channels: usize,
        producer: &mut HeapProd<f32>,
        overflow_counter: &AtomicU64,
    ) {
        for chunk in data.chunks(channels) {
            let sample = chunk[0] as f32 / 32768.0;
            if producer.try_push(sample).is_err() {
                overflow_counter.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }

    /// Handle u8 input samples (convert to f32)
    fn handle_input_u8(
        data: &[u8],
        channels: usize,
        producer: &mut HeapProd<f32>,
        overflow_counter: &AtomicU64,
    ) {
        for chunk in data.chunks(channels) {
            // u8 is unsigned: 0-255, with 128 as center
            let sample = (chunk[0] as f32 - 128.0) / 128.0;
            if producer.try_push(sample).is_err() {
                overflow_counter.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }

    /// Start capturing audio
    pub fn start(&self) -> Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        self.stream.play().context("Failed to start audio stream")?;
        info!("Audio capture started");
        Ok(())
    }

    /// Stop capturing audio
    pub fn stop(&self) -> Result<()> {
        self.is_running.store(false, Ordering::SeqCst);
        self.stream.pause().context("Failed to stop audio stream")?;
        info!("Audio capture stopped");
        Ok(())
    }

    /// Get the device sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the number of channels
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Get overflow count
    pub fn overflow_count(&self) -> u64 {
        self.overflow_counter.load(Ordering::Relaxed)
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_ring_buffer_capacity() {
        assert_eq!(calculate_ring_buffer_capacity(48000), 1_440_000);
        assert_eq!(calculate_ring_buffer_capacity(44100), 1_323_000);
        assert_eq!(calculate_ring_buffer_capacity(16000), 480_000);
    }

    #[test]
    fn test_list_devices() {
        // This test just checks that the function doesn't panic
        // Actual devices depend on the system
        let result = list_input_devices();
        // Don't assert success since CI might not have audio devices
        if let Ok(devices) = result {
            println!("Found {} input devices", devices.len());
        }
    }
}
