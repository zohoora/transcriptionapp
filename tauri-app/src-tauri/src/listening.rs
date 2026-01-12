//! Listening Mode Pipeline
//!
//! Lightweight audio monitoring for auto-session detection.
//! Runs VAD to detect sustained speech, then uses Whisper + LLM
//! to detect greetings that should start a new session.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use voice_activity_detector::VoiceActivityDetector;

use crate::audio::{list_input_devices, AudioResampler};
use crate::llm_client::LLMClient;
use crate::whisper_server::WhisperServerClient;

/// Target sample rate for VAD (16kHz)
const TARGET_SAMPLE_RATE: u32 = 16000;

/// VAD chunk size in samples (512 samples = 32ms at 16kHz)
const VAD_CHUNK_SIZE: usize = 512;

/// Ring buffer capacity in seconds
const BUFFER_SECONDS: f32 = 10.0;

/// Configuration for listening mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListeningConfig {
    /// VAD threshold (0.0 - 1.0)
    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,

    /// Minimum speech duration to trigger analysis (ms)
    #[serde(default = "default_min_speech_duration_ms")]
    pub min_speech_duration_ms: u32,

    /// Maximum audio buffer for greeting analysis (ms)
    #[serde(default = "default_max_buffer_ms")]
    pub max_buffer_ms: u32,

    /// Greeting detection confidence threshold
    #[serde(default = "default_greeting_sensitivity")]
    pub greeting_sensitivity: f32,

    /// Cooldown after non-greeting detection (ms)
    #[serde(default = "default_cooldown_ms")]
    pub cooldown_ms: u32,

    /// Whisper server URL
    pub whisper_server_url: String,

    /// Whisper model name
    pub whisper_server_model: String,

    /// LLM Router URL
    pub llm_router_url: String,

    /// LLM Router API key
    pub llm_api_key: String,

    /// LLM Client ID
    pub llm_client_id: String,

    /// Fast model for greeting detection
    pub fast_model: String,

    /// Language for transcription
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_vad_threshold() -> f32 {
    0.5
}
fn default_min_speech_duration_ms() -> u32 {
    2000
}
fn default_max_buffer_ms() -> u32 {
    5000
}
fn default_greeting_sensitivity() -> f32 {
    0.7
}
fn default_cooldown_ms() -> u32 {
    5000
}
fn default_language() -> String {
    "en".to_string()
}

impl Default for ListeningConfig {
    fn default() -> Self {
        Self {
            vad_threshold: default_vad_threshold(),
            min_speech_duration_ms: default_min_speech_duration_ms(),
            max_buffer_ms: default_max_buffer_ms(),
            greeting_sensitivity: default_greeting_sensitivity(),
            cooldown_ms: default_cooldown_ms(),
            whisper_server_url: "http://localhost:8000".to_string(),
            whisper_server_model: "large-v3-turbo".to_string(),
            llm_router_url: "http://localhost:4000".to_string(),
            llm_api_key: String::new(),
            llm_client_id: "ai-scribe".to_string(),
            fast_model: "fast-model".to_string(),
            language: default_language(),
        }
    }
}

/// Events emitted by the listening pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ListeningEvent {
    /// Listening mode started
    Started,

    /// Speech detected, accumulating
    SpeechDetected {
        duration_ms: u32,
    },

    /// Analyzing speech (Whisper + LLM)
    Analyzing,

    /// Start recording immediately (optimistic) - emitted before greeting check completes
    /// Frontend should start recording - initial audio is stored in backend state
    StartRecording {
        /// Audio samples at 16kHz mono to prepend to the recording
        /// Skipped during serialization - stored in backend state instead
        #[serde(skip)]
        initial_audio: Vec<f32>,
        /// Duration of the initial audio in milliseconds
        initial_audio_duration_ms: u32,
    },

    /// Greeting confirmed - recording should continue
    GreetingConfirmed {
        transcript: String,
        confidence: f32,
        detected_phrase: Option<String>,
    },

    /// Greeting rejected - recording should be discarded
    GreetingRejected {
        transcript: String,
        reason: String,
    },

    /// Greeting detected - should start session (legacy, for backward compatibility)
    GreetingDetected {
        transcript: String,
        confidence: f32,
        detected_phrase: Option<String>,
    },

    /// Not a greeting - continue listening
    NotGreeting {
        transcript: String,
    },

    /// Error occurred
    Error {
        message: String,
    },

    /// Listening mode stopped
    Stopped,
}

/// Internal commands for the listening thread
enum ListeningCommand {
    Stop,
}

/// Status of the listening pipeline
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListeningStatus {
    pub is_listening: bool,
    pub speech_detected: bool,
    pub speech_duration_ms: u32,
    pub analyzing: bool,
}

/// Handle to control the listening pipeline
pub struct ListeningHandle {
    /// Command channel
    cmd_tx: Sender<ListeningCommand>,
    /// Stop flag
    stop_flag: Arc<AtomicBool>,
    /// Thread handle
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl ListeningHandle {
    /// Request the listening thread to stop
    pub fn stop(&self) {
        info!("Requesting listening thread stop");
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.cmd_tx.send(ListeningCommand::Stop);
    }

    /// Wait for the thread to finish
    pub fn join(mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Check if the thread is still running
    pub fn is_running(&self) -> bool {
        !self.stop_flag.load(Ordering::SeqCst)
    }
}

impl Drop for ListeningHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.cmd_tx.send(ListeningCommand::Stop);
    }
}

/// Start the listening pipeline
pub fn start_listening<F>(
    config: ListeningConfig,
    device_id: Option<String>,
    event_callback: F,
) -> Result<ListeningHandle, String>
where
    F: Fn(ListeningEvent) + Send + 'static,
{
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    let thread_handle = thread::Builder::new()
        .name("listening-pipeline".to_string())
        .spawn(move || {
            if let Err(e) = run_listening_thread(config, device_id, cmd_rx, stop_flag_clone, event_callback) {
                error!("Listening thread error: {}", e);
            }
        })
        .map_err(|e| format!("Failed to spawn listening thread: {}", e))?;

    Ok(ListeningHandle {
        cmd_tx,
        stop_flag,
        thread_handle: Some(thread_handle),
    })
}

/// Main listening thread function
fn run_listening_thread<F>(
    config: ListeningConfig,
    device_id: Option<String>,
    cmd_rx: Receiver<ListeningCommand>,
    stop_flag: Arc<AtomicBool>,
    event_callback: F,
) -> Result<(), String>
where
    F: Fn(ListeningEvent) + Send + 'static,
{
    info!("Listening thread started");
    info!("  VAD threshold: {}", config.vad_threshold);
    info!("  Min speech duration: {}ms", config.min_speech_duration_ms);
    info!("  Greeting sensitivity: {}", config.greeting_sensitivity);

    // Emit started event
    event_callback(ListeningEvent::Started);

    // Find the audio device
    let devices = list_input_devices().map_err(|e| format!("Failed to list devices: {}", e))?;
    let device = if let Some(ref id) = device_id {
        devices
            .iter()
            .find(|d| d.id == *id)
            .ok_or_else(|| format!("Device not found: {}", id))?
    } else {
        devices
            .iter()
            .find(|d| d.is_default)
            .ok_or_else(|| "No default device found".to_string())?
    };

    info!("Using audio device: {}", device.name);

    // Set up audio capture with cpal
    let host = cpal::default_host();
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let cpal_device = if device.is_default {
        host.default_input_device()
            .ok_or_else(|| "No default input device".to_string())?
    } else {
        host.input_devices()
            .map_err(|e| format!("Failed to enumerate devices: {}", e))?
            .find(|d| d.name().map(|n| n == device.name).unwrap_or(false))
            .ok_or_else(|| format!("Device not found: {}", device.name))?
    };

    let supported_config = cpal_device
        .default_input_config()
        .map_err(|e| format!("Failed to get device config: {}", e))?;

    let input_sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels() as usize;
    info!(
        "Audio config: {}Hz, {} channels",
        input_sample_rate, channels
    );

    // Create ring buffer for audio
    let buffer_samples = (BUFFER_SECONDS * input_sample_rate as f32) as usize;
    let rb = HeapRb::<f32>::new(buffer_samples);
    let (mut producer, mut consumer) = rb.split();

    // Create audio stream
    let stream_config = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: cpal::SampleRate(input_sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = match supported_config.sample_format() {
        cpal::SampleFormat::F32 => cpal_device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &_| {
                // Mix to mono if stereo
                if channels == 1 {
                    for &sample in data {
                        let _ = producer.try_push(sample);
                    }
                } else {
                    for chunk in data.chunks(channels) {
                        let mono = chunk.iter().sum::<f32>() / channels as f32;
                        let _ = producer.try_push(mono);
                    }
                }
            },
            |err| error!("Audio stream error: {}", err),
            None,
        ),
        cpal::SampleFormat::I16 => cpal_device.build_input_stream(
            &stream_config,
            move |data: &[i16], _: &_| {
                if channels == 1 {
                    for &sample in data {
                        let f = sample as f32 / i16::MAX as f32;
                        let _ = producer.try_push(f);
                    }
                } else {
                    for chunk in data.chunks(channels) {
                        let mono = chunk.iter().map(|&s| s as f32 / i16::MAX as f32).sum::<f32>()
                            / channels as f32;
                        let _ = producer.try_push(mono);
                    }
                }
            },
            |err| error!("Audio stream error: {}", err),
            None,
        ),
        format => {
            return Err(format!("Unsupported sample format: {:?}", format));
        }
    }
    .map_err(|e| format!("Failed to build audio stream: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Failed to start audio stream: {}", e))?;

    // Create resampler
    let mut resampler = AudioResampler::new(input_sample_rate)
        .map_err(|e| format!("Failed to create resampler: {}", e))?;

    // Create VAD
    let mut vad = VoiceActivityDetector::builder()
        .sample_rate(TARGET_SAMPLE_RATE as i64)
        .chunk_size(VAD_CHUNK_SIZE)
        .build()
        .map_err(|e| format!("Failed to create VAD: {}", e))?;

    // Whisper client
    let whisper_client = WhisperServerClient::new(&config.whisper_server_url, &config.whisper_server_model)
        .map_err(|e| format!("Failed to create Whisper client: {}", e))?;

    // LLM client for greeting detection
    let llm_client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id)
        .map_err(|e| format!("Failed to create LLM client: {}", e))?;

    // State variables
    let mut staging_buffer: Vec<f32> = Vec::new();
    let mut speech_buffer: Vec<f32> = Vec::new();
    let mut is_speech_active = false;
    let mut speech_start_time: Option<Instant> = None;
    let mut last_analysis_time: Option<Instant> = None;
    // The AudioResampler uses rubato::SincFixedIn with chunk_size=1024
    let input_chunk_size = resampler.input_frames_next();
    let mut input_buffer = vec![0.0f32; input_chunk_size];

    let min_speech_samples = (config.min_speech_duration_ms as f32 / 1000.0 * TARGET_SAMPLE_RATE as f32) as usize;
    let max_buffer_samples = (config.max_buffer_ms as f32 / 1000.0 * TARGET_SAMPLE_RATE as f32) as usize;
    let cooldown_duration = Duration::from_millis(config.cooldown_ms as u64);

    info!("Listening loop starting...");

    // Main listening loop
    loop {
        // Check for stop command
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        if let Ok(ListeningCommand::Stop) = cmd_rx.try_recv() {
            break;
        }

        // Check cooldown
        if let Some(last_time) = last_analysis_time {
            if last_time.elapsed() < cooldown_duration {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        }

        // Read available audio
        let available = consumer.occupied_len();
        if available < input_buffer.len() {
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        // Read and resample
        let read = std::cmp::min(available, input_buffer.len());
        for i in 0..read {
            if let Some(sample) = consumer.try_pop() {
                input_buffer[i] = sample;
            }
        }

        let resampled = match resampler.process(&input_buffer[..read]) {
            Ok(samples) => samples,
            Err(e) => {
                warn!("Resampling error: {}", e);
                continue;
            }
        };

        // Add to staging buffer
        staging_buffer.extend_from_slice(&resampled);

        // Process VAD chunks
        while staging_buffer.len() >= VAD_CHUNK_SIZE {
            let chunk: Vec<f32> = staging_buffer.drain(..VAD_CHUNK_SIZE).collect();

            // Run VAD
            let speech_prob = vad.predict(chunk.iter().copied());
            let is_speech = speech_prob > config.vad_threshold;

            if is_speech && !is_speech_active {
                // Speech started
                is_speech_active = true;
                speech_start_time = Some(Instant::now());
                speech_buffer.clear();
                debug!("Speech started");
            }

            if is_speech_active {
                // Accumulate speech
                speech_buffer.extend_from_slice(&chunk);

                // Check duration - continue if no start time recorded
                let Some(start_time) = speech_start_time else {
                    continue;
                };

                let duration_ms = start_time.elapsed().as_millis() as u32;

                // Emit speech detected event periodically
                if duration_ms % 500 < 50 {
                    event_callback(ListeningEvent::SpeechDetected { duration_ms });
                }

                // Wait until we have enough speech to analyze
                if speech_buffer.len() < min_speech_samples {
                    continue;
                }

                info!("Enough speech accumulated ({}ms), starting recording and analyzing...", duration_ms);

                // Cap the buffer to max size
                if speech_buffer.len() > max_buffer_samples {
                    speech_buffer = speech_buffer[speech_buffer.len() - max_buffer_samples..].to_vec();
                }

                // Calculate duration of initial audio
                let initial_audio_duration_ms = (speech_buffer.len() as f32 / TARGET_SAMPLE_RATE as f32 * 1000.0) as u32;

                // OPTIMISTIC RECORDING: Start recording immediately BEFORE greeting check
                // This prevents losing audio during the ~35s LLM check
                info!("Emitting StartRecording with {}ms of initial audio", initial_audio_duration_ms);
                event_callback(ListeningEvent::StartRecording {
                    initial_audio: speech_buffer.clone(),
                    initial_audio_duration_ms,
                });

                // Analyze speech (blocking - transcribe then check greeting)
                // Recording is already started, so no audio is lost during this check
                let analysis_result = analyze_speech(&speech_buffer, &whisper_client, &llm_client, &config);

                // Check if stop was requested while we were analyzing
                if stop_flag.load(Ordering::SeqCst) {
                    info!("Stop requested during analysis - not emitting greeting result");
                    break;
                }

                match analysis_result {
                    Ok(result) if result.is_greeting && result.confidence >= config.greeting_sensitivity => {
                        info!(
                            "Greeting confirmed: '{}' (confidence: {:.2})",
                            result.detected_phrase.as_deref().unwrap_or(""),
                            result.confidence
                        );
                        // Emit both GreetingConfirmed (new) and GreetingDetected (legacy)
                        event_callback(ListeningEvent::GreetingConfirmed {
                            transcript: result.transcript.clone(),
                            confidence: result.confidence,
                            detected_phrase: result.detected_phrase.clone(),
                        });
                        event_callback(ListeningEvent::GreetingDetected {
                            transcript: result.transcript,
                            confidence: result.confidence,
                            detected_phrase: result.detected_phrase,
                        });
                        break; // Stop after greeting confirmed
                    }
                    Ok(result) => {
                        info!("Not a greeting, rejecting: '{}'", result.transcript);
                        event_callback(ListeningEvent::GreetingRejected {
                            transcript: result.transcript.clone(),
                            reason: "Speech did not match greeting patterns".to_string(),
                        });
                        event_callback(ListeningEvent::NotGreeting {
                            transcript: result.transcript,
                        });
                        last_analysis_time = Some(Instant::now());
                    }
                    Err(e) => {
                        warn!("Analysis error: {}", e);
                        event_callback(ListeningEvent::GreetingRejected {
                            transcript: String::new(),
                            reason: e.to_string(),
                        });
                        event_callback(ListeningEvent::Error {
                            message: e.to_string(),
                        });
                        last_analysis_time = Some(Instant::now());
                    }
                }

                // Reset state
                is_speech_active = false;
                speech_start_time = None;
                speech_buffer.clear();
            }

            if !is_speech && is_speech_active {
                // Allow some silence within speech (500ms)
                if let Some(start_time) = speech_start_time {
                    if start_time.elapsed() > Duration::from_millis(500) && speech_buffer.len() < min_speech_samples / 2 {
                        // Too short, reset
                        debug!("Speech too short, resetting");
                        is_speech_active = false;
                        speech_start_time = None;
                        speech_buffer.clear();
                    }
                }
            }
        }

        // Check if greeting was detected (break out of outer loop)
        if !is_speech_active && last_analysis_time.is_none() && speech_buffer.is_empty() {
            // Continue listening
        }
    }

    info!("Listening thread stopped");
    event_callback(ListeningEvent::Stopped);
    Ok(())
}

/// Result of speech analysis
struct AnalysisResult {
    transcript: String,
    is_greeting: bool,
    confidence: f32,
    detected_phrase: Option<String>,
}

/// Analyze speech buffer for greeting detection
fn analyze_speech(
    audio: &[f32],
    whisper_client: &WhisperServerClient,
    llm_client: &LLMClient,
    config: &ListeningConfig,
) -> Result<AnalysisResult, String> {
    // Create a runtime for async operations
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to create runtime: {}", e))?;

    rt.block_on(async {
        // Step 1: Transcribe with Whisper
        info!("Transcribing {} samples...", audio.len());
        let transcript = whisper_client
            .transcribe(audio, &config.language)
            .await
            .map_err(|e| format!("Transcription error: {}", e))?;

        if transcript.trim().is_empty() {
            return Ok(AnalysisResult {
                transcript: String::new(),
                is_greeting: false,
                confidence: 0.0,
                detected_phrase: None,
            });
        }

        info!("Transcript: '{}'", transcript);

        // Step 2: Check for greeting with LLM router
        info!("Checking for greeting...");
        let greeting_result = llm_client
            .check_greeting(&transcript, config.greeting_sensitivity)
            .await
            .map_err(|e| format!("Greeting check error: {}", e))?;

        Ok(AnalysisResult {
            transcript,
            is_greeting: greeting_result.is_greeting,
            confidence: greeting_result.confidence,
            detected_phrase: greeting_result.detected_phrase,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listening_config_defaults() {
        let config = ListeningConfig::default();
        assert_eq!(config.vad_threshold, 0.5);
        assert_eq!(config.min_speech_duration_ms, 2000);
        assert_eq!(config.greeting_sensitivity, 0.7);
    }
}
