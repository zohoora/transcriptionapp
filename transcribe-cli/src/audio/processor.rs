use anyhow::Result;
use ringbuf::traits::{Consumer as ConsumerTrait, Observer};
use ringbuf::HeapCons;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use voice_activity_detector::VoiceActivityDetector;

use super::resampler::AudioResampler;
use crate::transcription::Utterance;
use crate::vad::{VadConfig, VadGatedPipeline};

/// VAD chunk size at 16kHz
const VAD_CHUNK_SIZE: usize = 512;

/// Message from processing thread
#[derive(Debug)]
pub enum ProcessorMessage {
    /// New utterance ready for transcription
    Utterance(Utterance),
    /// Processing status update
    Status {
        audio_clock_ms: u64,
        pending_count: usize,
        is_speech_active: bool,
    },
    /// Processing thread stopped
    Stopped,
    /// Error occurred
    Error(String),
}

/// Audio processing thread configuration
pub struct ProcessorConfig {
    pub device_sample_rate: u32,
    pub vad_config: VadConfig,
    pub status_interval_ms: u64,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            device_sample_rate: 48000,
            vad_config: VadConfig::default(),
            status_interval_ms: 500,
        }
    }
}

/// Run the audio processing thread
///
/// This function should be spawned in a separate thread/task.
/// It reads from the ring buffer, resamples, runs VAD, and sends
/// utterances for transcription.
pub fn run_processor(
    mut consumer: HeapCons<f32>,
    config: ProcessorConfig,
    tx: mpsc::Sender<ProcessorMessage>,
    stop_flag: Arc<AtomicBool>,
) {
    let result = run_processor_inner(&mut consumer, config, &tx, stop_flag);

    if let Err(e) = result {
        let _ = tx.blocking_send(ProcessorMessage::Error(e.to_string()));
    }

    let _ = tx.blocking_send(ProcessorMessage::Stopped);
}

fn run_processor_inner(
    consumer: &mut HeapCons<f32>,
    config: ProcessorConfig,
    tx: &mpsc::Sender<ProcessorMessage>,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    info!(
        "Starting audio processor: {} Hz input, VAD threshold: {}",
        config.device_sample_rate, config.vad_config.vad_threshold
    );

    // Create resampler
    let mut resampler = AudioResampler::new(config.device_sample_rate)?;

    // Create VAD
    let mut vad = VoiceActivityDetector::builder()
        .sample_rate(16000)
        .chunk_size(VAD_CHUNK_SIZE)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create VAD: {:?}", e))?;

    // Create VAD pipeline
    let mut pipeline = VadGatedPipeline::with_config(config.vad_config);

    // Staging buffer for VAD chunks
    let mut staging_buffer: Vec<f32> = Vec::with_capacity(VAD_CHUNK_SIZE * 2);

    // Input buffer for resampler
    let input_frames = resampler.input_frames_next();
    let mut input_buffer = vec![0.0f32; input_frames];

    // Status tracking
    let mut last_status_time = std::time::Instant::now();
    let status_interval = Duration::from_millis(config.status_interval_ms);

    info!("Audio processor started, waiting for audio data...");

    loop {
        // Check stop flag
        if stop_flag.load(Ordering::Relaxed) {
            info!("Stop flag received, flushing pipeline...");
            pipeline.force_flush();

            // Send any remaining utterances
            while let Some(utterance) = pipeline.pop_utterance() {
                if tx.blocking_send(ProcessorMessage::Utterance(utterance)).is_err() {
                    break;
                }
            }
            break;
        }

        // Wait for enough raw samples
        let available = consumer.occupied_len();
        if available < input_frames {
            std::thread::sleep(Duration::from_millis(5));

            // Send status updates periodically
            if last_status_time.elapsed() >= status_interval {
                let _ = tx.blocking_send(ProcessorMessage::Status {
                    audio_clock_ms: pipeline.audio_clock_ms(),
                    pending_count: pipeline.pending_count(),
                    is_speech_active: pipeline.is_speech_active(),
                });
                last_status_time = std::time::Instant::now();
            }

            continue;
        }

        // Read from ring buffer
        let read = consumer.pop_slice(&mut input_buffer);
        if read < input_frames {
            warn!("Incomplete read from ring buffer: {} < {}", read, input_frames);
            continue;
        }

        // Resample to 16kHz
        let resampled = resampler.process(&input_buffer)?;

        // Accumulate into staging buffer
        staging_buffer.extend_from_slice(&resampled);

        // Process complete VAD chunks
        while staging_buffer.len() >= VAD_CHUNK_SIZE {
            let chunk: Vec<f32> = staging_buffer.drain(..VAD_CHUNK_SIZE).collect();

            // Advance audio clock by chunk size (in 16kHz samples)
            pipeline.advance_audio_clock(VAD_CHUNK_SIZE);

            // VAD + accumulation
            pipeline.process_chunk(&chunk, &mut vad);
        }

        // Send any ready utterances
        while let Some(utterance) = pipeline.pop_utterance() {
            debug!(
                "Sending utterance for transcription: {}ms - {}ms",
                utterance.start_ms, utterance.end_ms
            );
            if tx.blocking_send(ProcessorMessage::Utterance(utterance)).is_err() {
                warn!("Failed to send utterance, receiver dropped");
                return Ok(());
            }
        }

        // Send status updates periodically
        if last_status_time.elapsed() >= status_interval {
            let _ = tx.blocking_send(ProcessorMessage::Status {
                audio_clock_ms: pipeline.audio_clock_ms(),
                pending_count: pipeline.pending_count(),
                is_speech_active: pipeline.is_speech_active(),
            });
            last_status_time = std::time::Instant::now();
        }
    }

    info!("Audio processor stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_config_default() {
        let config = ProcessorConfig::default();
        assert_eq!(config.device_sample_rate, 48000);
        assert_eq!(config.status_interval_ms, 500);
    }
}
