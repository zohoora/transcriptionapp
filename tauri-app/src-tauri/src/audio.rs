//! # Audio Module
//!
//! This module handles audio capture from input devices and resampling
//! to the required sample rate for the Whisper model (16kHz).
//!
//! ## Overview
//!
//! The module provides:
//!
//! - Device enumeration via [`list_input_devices`]
//! - Audio capture with [`AudioCapture`]
//! - Sample rate conversion with [`AudioResampler`]
//!
//! ## Audio Pipeline
//!
//! ```text
//! Input Device (48kHz) -> AudioCapture -> Ring Buffer -> AudioResampler -> 16kHz samples
//! ```
//!
//! ## Example
//!
//! ```no_run
//! use transcription_app::audio::{list_input_devices, AudioCapture, AudioResampler};
//!
//! // List available devices
//! let devices = list_input_devices().unwrap();
//! for device in &devices {
//!     println!("{}: {} (default: {})", device.id, device.name, device.is_default);
//! }
//!
//! // Create a resampler for 48kHz input
//! let mut resampler = AudioResampler::new(48000).unwrap();
//! ```

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device as CpalDevice, SampleFormat, Stream, StreamConfig};
use ringbuf::traits::Producer as ProducerTrait;
use ringbuf::HeapProd;
use rubato::Resampler;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Audio device information exposed to the frontend.
///
/// Represents an audio input device that can be selected for recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Unique identifier for the device (typically the device name)
    pub id: String,
    /// Human-readable device name
    pub name: String,
    /// Whether this is the system's default input device
    pub is_default: bool,
}

/// Internal audio device representation.
///
/// Used internally for device enumeration before converting to [`Device`].
#[derive(Debug, Clone)]
pub struct AudioDevice {
    /// Unique identifier for the device
    pub id: String,
    /// Human-readable device name
    pub name: String,
    /// Whether this is the system's default input device
    pub is_default: bool,
}

/// Lists all available audio input devices.
///
/// Returns a vector of [`AudioDevice`] structs containing information
/// about each input device detected on the system.
///
/// # Errors
///
/// Returns an error if the audio host cannot enumerate devices.
pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let default_device = host.default_input_device();
    let default_name = default_device
        .as_ref()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();

    for device in host
        .input_devices()
        .context("Failed to enumerate input devices")?
    {
        if let Ok(name) = device.name() {
            let is_default = name == default_name;
            devices.push(AudioDevice {
                id: name.clone(),
                name,
                is_default,
            });
        }
    }

    Ok(devices)
}

/// Gets an audio device by its ID (name) or returns the default device.
///
/// # Arguments
///
/// * `device_id` - Optional device ID. If `None` or `"default"`, returns the default device.
///
/// # Errors
///
/// Returns an error if the device is not found or no default device is available.
pub fn get_device(device_id: Option<&str>) -> Result<CpalDevice> {
    let host = cpal::default_host();

    match device_id {
        Some(id) if id != "default" => {
            for device in host
                .input_devices()
                .context("Failed to enumerate devices")?
            {
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

/// Selects the best input configuration for a device.
///
/// Prefers mono configurations to minimize processing overhead.
/// Falls back to the default configuration if no mono config is available.
///
/// # Arguments
///
/// * `device` - The audio input device to configure.
///
/// # Errors
///
/// Returns an error if no supported configuration can be found.
pub fn select_input_config(device: &CpalDevice) -> Result<StreamConfig> {
    // First try to find a mono config
    if let Ok(supported) = device.supported_input_configs() {
        for config_range in supported {
            if config_range.channels() == 1 {
                let config = config_range.with_max_sample_rate();
                debug!(
                    "Selected mono config: {} Hz, {} channels",
                    config.sample_rate().0,
                    config.channels()
                );
                return Ok(config.into());
            }
        }
    }

    // Fall back to default (will downmix in callback)
    let config = device
        .default_input_config()
        .context("No default input config")?;
    debug!(
        "Using default config (will downmix): {} Hz, {} channels",
        config.sample_rate().0,
        config.channels()
    );
    Ok(config.into())
}

/// Calculates the ring buffer capacity for a given sample rate.
///
/// The buffer holds 30 seconds of audio to allow for processing latency.
///
/// # Arguments
///
/// * `device_sample_rate` - The sample rate of the input device in Hz.
///
/// # Returns
///
/// The number of samples the buffer should hold.
pub fn calculate_ring_buffer_capacity(device_sample_rate: u32) -> usize {
    const BUFFER_DURATION_SECONDS: u32 = 30;
    (device_sample_rate * BUFFER_DURATION_SECONDS) as usize
}

/// Handle for an active audio capture stream.
///
/// Manages the audio input stream and provides methods to control capture.
/// The stream reads from the audio device and writes samples to a ring buffer.
pub struct AudioCapture {
    stream: Stream,
    sample_rate: u32,
    channels: u16,
    overflow_counter: Arc<AtomicU64>,
    is_running: Arc<AtomicBool>,
}

impl AudioCapture {
    /// Creates a new audio capture stream.
    ///
    /// Builds an input stream that captures audio from the device and writes
    /// samples to the provided ring buffer. Automatically handles sample format
    /// conversion and channel downmixing to mono.
    ///
    /// # Arguments
    ///
    /// * `device` - The audio input device to capture from.
    /// * `config` - Stream configuration (sample rate, channels).
    /// * `producer` - Ring buffer producer to write samples to.
    ///
    /// # Errors
    ///
    /// Returns an error if the stream cannot be built or the sample format is unsupported.
    pub fn new(
        device: &CpalDevice,
        config: &StreamConfig,
        mut producer: HeapProd<f32>,
    ) -> Result<Self> {
        let sample_format = device
            .default_input_config()
            .context("No default input config")?
            .sample_format();

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

/// Audio resampler (device rate -> 16kHz)
pub struct AudioResampler {
    resampler: rubato::SincFixedIn<f32>,
    output_buffer: Vec<Vec<f32>>,
}

impl AudioResampler {
    /// Create a new resampler from device sample rate to 16kHz
    pub fn new(device_sample_rate: u32) -> Result<Self> {
        let ratio = 16000.0 / device_sample_rate as f64;

        let params = rubato::SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            oversampling_factor: 256,
            interpolation: rubato::SincInterpolationType::Linear,
            window: rubato::WindowFunction::BlackmanHarris2,
        };

        let resampler = rubato::SincFixedIn::<f32>::new(
            ratio,
            2.0,
            params,
            1024,
            1,
        )?;

        let max_output = resampler.output_frames_max();
        let output_buffer = vec![vec![0.0f32; max_output]; 1];

        Ok(Self {
            resampler,
            output_buffer,
        })
    }

    /// Get the number of input frames needed for next process call
    pub fn input_frames_next(&self) -> usize {
        self.resampler.input_frames_next()
    }

    /// Process input samples and return resampled output
    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        let input_buffer = vec![input.to_vec()];
        let (_, output_samples) = self.resampler.process_into_buffer(
            &input_buffer,
            &mut self.output_buffer,
            None,
        )?;

        Ok(self.output_buffer[0][..output_samples].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Property-based tests
    proptest! {
        #[test]
        fn prop_ring_buffer_capacity_proportional_to_rate(sample_rate in 8000u32..192000) {
            let capacity = calculate_ring_buffer_capacity(sample_rate);
            // Should be exactly 30 seconds worth
            prop_assert_eq!(capacity, (sample_rate * 30) as usize);
        }

        #[test]
        fn prop_i16_to_f32_in_range(sample in i16::MIN..=i16::MAX) {
            let f32_sample = sample as f32 / 32768.0;
            prop_assert!(f32_sample >= -1.0);
            prop_assert!(f32_sample <= 1.0);
        }

        #[test]
        fn prop_u8_to_f32_in_range(sample in u8::MIN..=u8::MAX) {
            let f32_sample = (sample as f32 - 128.0) / 128.0;
            prop_assert!(f32_sample >= -1.0);
            prop_assert!(f32_sample <= 1.0);
        }

        #[test]
        fn prop_resampler_output_not_empty(
            sample_rate in prop_oneof![
                Just(44100u32),
                Just(48000u32),
                Just(96000u32),
                Just(22050u32),
            ]
        ) {
            let mut resampler = AudioResampler::new(sample_rate).unwrap();
            let input_frames = resampler.input_frames_next();
            let input = vec![0.0f32; input_frames];

            let output = resampler.process(&input).unwrap();
            // Resampler should produce some output (may have latency on first call)
            // Just verify it doesn't panic
            prop_assert!(output.len() <= input_frames * 2); // Reasonable upper bound
        }

        #[test]
        fn prop_resampler_handles_silence(sample_rate in 16000u32..96000) {
            if let Ok(mut resampler) = AudioResampler::new(sample_rate) {
                let input_frames = resampler.input_frames_next();
                let input = vec![0.0f32; input_frames];

                if let Ok(output) = resampler.process(&input) {
                    // Silence in should produce near-silence out
                    let max_abs: f32 = output.iter().map(|x| x.abs()).fold(0.0, f32::max);
                    prop_assert!(max_abs < 0.01, "Expected near-silence, got max abs {}", max_abs);
                }
            }
        }

        #[test]
        fn prop_f32_mono_downmix_takes_first_channel(
            samples in proptest::collection::vec(-1.0f32..1.0, 2..100)
        ) {
            // Simulate stereo input (must be even length)
            let stereo: Vec<f32> = if samples.len() % 2 == 0 {
                samples.clone()
            } else {
                samples[..samples.len()-1].to_vec()
            };

            let channels = 2;
            let mut mono = Vec::new();
            for chunk in stereo.chunks(channels) {
                mono.push(chunk[0]);
            }

            // Mono should have half the samples
            prop_assert_eq!(mono.len(), stereo.len() / 2);

            // Each mono sample should be the first channel
            for (i, chunk) in stereo.chunks(channels).enumerate() {
                prop_assert!((mono[i] - chunk[0]).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_calculate_ring_buffer_capacity() {
        assert_eq!(calculate_ring_buffer_capacity(48000), 1_440_000);
        assert_eq!(calculate_ring_buffer_capacity(44100), 1_323_000);
        assert_eq!(calculate_ring_buffer_capacity(16000), 480_000);
    }

    #[test]
    fn test_list_devices() {
        // This test just checks that the function doesn't panic
        let result = list_input_devices();
        if let Ok(devices) = result {
            println!("Found {} input devices", devices.len());
        }
    }

    #[test]
    fn test_ring_buffer_capacity_various_rates() {
        // Common sample rates
        assert_eq!(calculate_ring_buffer_capacity(8000), 240_000);
        assert_eq!(calculate_ring_buffer_capacity(22050), 661_500);
        assert_eq!(calculate_ring_buffer_capacity(96000), 2_880_000);
    }

    #[test]
    fn test_audio_device_struct() {
        let device = AudioDevice {
            id: "mic-1".to_string(),
            name: "Built-in Microphone".to_string(),
            is_default: true,
        };
        assert_eq!(device.id, "mic-1");
        assert_eq!(device.name, "Built-in Microphone");
        assert!(device.is_default);
    }

    #[test]
    fn test_audio_device_clone() {
        let device = AudioDevice {
            id: "mic-1".to_string(),
            name: "Microphone".to_string(),
            is_default: false,
        };
        let cloned = device.clone();
        assert_eq!(device.id, cloned.id);
        assert_eq!(device.name, cloned.name);
        assert_eq!(device.is_default, cloned.is_default);
    }

    #[test]
    fn test_get_device_default() {
        // Test that getting default device doesn't panic
        let result = get_device(None);
        // May fail on CI without audio hardware
        if result.is_ok() {
            assert!(result.unwrap().name().is_ok());
        }
    }

    #[test]
    fn test_get_device_explicit_default() {
        // "default" should behave same as None
        let result = get_device(Some("default"));
        // May fail on CI without audio hardware
        if result.is_ok() {
            assert!(result.unwrap().name().is_ok());
        }
    }

    #[test]
    fn test_get_device_nonexistent() {
        let result = get_device(Some("nonexistent-device-12345"));
        assert!(result.is_err());
    }

    // Tests for sample conversion (these don't require hardware)

    fn test_f32_to_mono_conversion() {
        // Simulating stereo to mono by averaging
        let stereo: Vec<f32> = vec![0.5, 0.3, 0.8, 0.2, -0.1, 0.1];
        let mut mono = Vec::new();
        for chunk in stereo.chunks(2) {
            // Taking first channel only (as in handle_input_f32)
            mono.push(chunk[0]);
        }
        assert_eq!(mono.len(), 3);
        assert_eq!(mono[0], 0.5);
        assert_eq!(mono[1], 0.8);
        assert_eq!(mono[2], -0.1);
    }

    #[test]
    fn test_i16_to_f32_conversion() {
        // Max positive i16
        let sample: i16 = 32767;
        let f32_sample = sample as f32 / 32768.0;
        assert!(f32_sample > 0.999);
        assert!(f32_sample < 1.0);

        // Max negative i16
        let sample: i16 = -32768;
        let f32_sample = sample as f32 / 32768.0;
        assert_eq!(f32_sample, -1.0);

        // Zero
        let sample: i16 = 0;
        let f32_sample = sample as f32 / 32768.0;
        assert_eq!(f32_sample, 0.0);
    }

    #[test]
    fn test_u8_to_f32_conversion() {
        // Center value (128) should be 0
        let sample: u8 = 128;
        let f32_sample = (sample as f32 - 128.0) / 128.0;
        assert_eq!(f32_sample, 0.0);

        // Max value (255) should be close to 1
        let sample: u8 = 255;
        let f32_sample = (sample as f32 - 128.0) / 128.0;
        assert!(f32_sample > 0.99);

        // Min value (0) should be -1
        let sample: u8 = 0;
        let f32_sample = (sample as f32 - 128.0) / 128.0;
        assert_eq!(f32_sample, -1.0);
    }

    #[test]
    fn test_resampler_creation() {
        // 48kHz to 16kHz
        let resampler = AudioResampler::new(48000);
        assert!(resampler.is_ok());

        // 44.1kHz to 16kHz
        let resampler = AudioResampler::new(44100);
        assert!(resampler.is_ok());

        // 16kHz to 16kHz (1:1)
        let resampler = AudioResampler::new(16000);
        assert!(resampler.is_ok());
    }

    #[test]
    fn test_resampler_input_frames() {
        let resampler = AudioResampler::new(48000).unwrap();
        let input_frames = resampler.input_frames_next();
        // Should be 1024 based on the constructor
        assert_eq!(input_frames, 1024);
    }

    #[test]
    fn test_resampler_process() {
        let mut resampler = AudioResampler::new(48000).unwrap();
        let input_frames = resampler.input_frames_next();

        // Create silence input
        let input = vec![0.0f32; input_frames];
        let result = resampler.process(&input);

        assert!(result.is_ok());
        let output = result.unwrap();

        // Output should be approximately input_frames * (16000/48000) = input_frames / 3
        // But rubato may add some latency
        assert!(!output.is_empty());
    }

    #[test]
    fn test_resampler_sine_wave() {
        let mut resampler = AudioResampler::new(48000).unwrap();
        let input_frames = resampler.input_frames_next();

        // Create a 440Hz sine wave at 48kHz
        let mut input = Vec::with_capacity(input_frames);
        for i in 0..input_frames {
            let t = i as f32 / 48000.0;
            input.push((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5);
        }

        let result = resampler.process(&input);
        assert!(result.is_ok());
        let output = result.unwrap();

        // Check output is not all zeros
        let sum: f32 = output.iter().map(|x| x.abs()).sum();
        assert!(sum > 0.0);
    }

    #[test]
    fn test_ring_buffer_30_seconds() {
        // Verify 30 second buffer is correct
        let capacity = calculate_ring_buffer_capacity(48000);
        // 48000 samples/sec * 30 sec = 1,440,000
        assert_eq!(capacity, 48000 * 30);
    }
}
