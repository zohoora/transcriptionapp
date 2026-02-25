use anyhow::Result;
use cpal::traits::DeviceTrait;
use hound::{WavSpec, WavWriter};
use ringbuf::traits::{Consumer as ConsumerTrait, Observer, Split};
use ringbuf::HeapRb;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use voice_activity_detector::VoiceActivityDetector;

use crate::audio::{calculate_ring_buffer_capacity, get_device, select_input_config, AudioCapture, AudioResampler};
use crate::preprocessing::AudioPreprocessor;
use crate::transcription::{Segment, Utterance};
use crate::vad::{VadConfig, VadGatedPipeline};
use crate::whisper_server::WhisperServerClient;

#[cfg(feature = "diarization")]
use crate::diarization::{DiarizationConfig, DiarizationProvider};

#[cfg(feature = "diarization")]
use crate::speaker_profiles::{SpeakerProfile, SpeakerProfileManager};

#[cfg(feature = "enhancement")]
use crate::enhancement::{EnhancementConfig, EnhancementProvider};

use crate::biomarkers::{AudioQualitySnapshot, BiomarkerConfig, BiomarkerHandle, BiomarkerOutput, BiomarkerUpdate, CoughEvent, start_biomarker_thread};
use std::collections::{HashSet, VecDeque};

/// VAD chunk size at 16kHz
const VAD_CHUNK_SIZE: usize = 512;

/// Transcribe an utterance via the STT server streaming endpoint and return a segment.
///
/// Uses WebSocket streaming to get partial transcript chunks in real-time.
/// Falls back to batch transcription if streaming fails.
fn transcribe_utterance(
    client: &WhisperServerClient,
    utterance: &Utterance,
    _language: &str,
    stt_alias: &str,
    stt_postprocess: bool,
    tx: &tokio::sync::mpsc::Sender<PipelineMessage>,
) -> Result<Segment, String> {
    // Use streaming transcription with chunk callback
    let tx_clone = tx.clone();
    let text = client.transcribe_streaming_blocking(
        &utterance.audio,
        stt_alias,
        stt_postprocess,
        |chunk_text| {
            // Emit partial transcript chunk to frontend
            let _ = tx_clone.blocking_send(PipelineMessage::TranscriptChunk {
                text: chunk_text.to_string(),
            });
        },
    )?;

    Ok(Segment::new(
        utterance.start_ms,
        utterance.end_ms,
        text,
    ))
}

/// Message from the transcription pipeline to the session controller
#[derive(Debug)]
#[allow(dead_code)] // Fields used for debugging and future UI features
pub enum PipelineMessage {
    /// New segment transcribed
    Segment(Segment),
    /// Partial transcript chunk from streaming STT (for real-time display)
    TranscriptChunk {
        text: String,
    },
    /// Processing status update
    Status {
        audio_clock_ms: u64,
        pending_count: usize,
        is_speech_active: bool,
    },
    /// Biomarker update for frontend
    Biomarker(BiomarkerUpdate),
    /// Audio quality update for frontend
    AudioQuality(AudioQualitySnapshot),
    /// Auto-end due to continuous silence detected
    AutoEndSilence {
        /// Duration of continuous silence in milliseconds
        silence_duration_ms: u64,
    },
    /// Warning: silence detected, auto-end approaching
    SilenceWarning {
        /// Milliseconds of silence so far
        silence_ms: u64,
        /// Milliseconds remaining until auto-end
        remaining_ms: u64,
    },
    /// Native STT shadow transcript (accumulated from Apple SFSpeechRecognizer)
    NativeSttShadowTranscript {
        transcript: String,
    },
    /// Pipeline has stopped
    Stopped,
    /// Error occurred
    Error(String),
}

/// Pipeline configuration
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub device_id: Option<String>,
    pub model_path: PathBuf,
    pub language: String,
    pub vad_threshold: f32,
    pub silence_to_flush_ms: u32,
    pub max_utterance_ms: u32,
    // Diarization settings
    pub diarization_enabled: bool,
    pub diarization_model_path: Option<PathBuf>,
    pub speaker_similarity_threshold: f32,
    pub max_speakers: usize,
    // Enhancement settings
    #[allow(dead_code)]
    pub enhancement_enabled: bool,
    #[allow(dead_code)]
    pub enhancement_model_path: Option<PathBuf>,
    // Biomarker analysis settings
    pub biomarkers_enabled: bool,
    pub yamnet_model_path: Option<PathBuf>,
    // Audio recording settings
    pub audio_output_path: Option<PathBuf>,
    // Audio preprocessing settings
    pub preprocessing_enabled: bool,
    pub preprocessing_highpass_hz: u32,
    pub preprocessing_agc_target_rms: f32,
    // STT server settings (for remote transcription)
    pub whisper_server_url: String,
    pub whisper_server_model: String,
    /// STT alias to use for streaming transcription (e.g., "medical-streaming")
    pub stt_alias: String,
    /// Whether to enable medical term post-processing on STT results
    pub stt_postprocess: bool,
    // Initial audio buffer from listening mode (optimistic recording)
    // This buffer contains audio captured before the greeting check completed
    // and should be prepended to the recording at startup
    pub initial_audio_buffer: Option<Vec<f32>>,
    // Auto-end settings
    pub auto_end_enabled: bool,
    pub auto_end_silence_ms: u64,
    // Native STT shadow (Apple SFSpeechRecognizer comparison)
    pub native_stt_shadow_enabled: bool,
}

impl PipelineConfig {
    /// Build a PipelineConfig from a Config, resolving model paths.
    ///
    /// Caller provides the mode-specific overrides:
    /// - `device_id`: audio device (session mode maps "default" to None)
    /// - `audio_output_path`: WAV output path (session uses "session_*.wav", continuous uses "continuous_*.wav")
    /// - `initial_audio_buffer`: pre-recorded audio from listening mode (session only)
    /// - `auto_end_enabled` / `auto_end_silence_ms`: auto-end silence (disabled in continuous mode)
    pub fn from_config(
        config: &crate::config::Config,
        device_id: Option<String>,
        audio_output_path: Option<std::path::PathBuf>,
        initial_audio_buffer: Option<Vec<f32>>,
        auto_end_enabled: bool,
        auto_end_silence_ms: u64,
    ) -> Self {
        let model_path = config.get_model_path().unwrap_or_default();
        let diarization_model_path = if config.diarization_enabled {
            config.get_diarization_model_path().ok()
        } else {
            None
        };
        let enhancement_model_path = if config.enhancement_enabled {
            config.get_enhancement_model_path().ok()
        } else {
            None
        };
        let yamnet_model_path = if config.biomarkers_enabled {
            config.get_yamnet_model_path().ok()
        } else {
            None
        };

        Self {
            device_id,
            model_path,
            language: config.language.clone(),
            vad_threshold: config.vad_threshold,
            silence_to_flush_ms: config.silence_to_flush_ms,
            max_utterance_ms: config.max_utterance_ms,
            diarization_enabled: config.diarization_enabled,
            diarization_model_path,
            speaker_similarity_threshold: config.speaker_similarity_threshold,
            max_speakers: config.max_speakers,
            enhancement_enabled: config.enhancement_enabled,
            enhancement_model_path,
            biomarkers_enabled: config.biomarkers_enabled,
            yamnet_model_path,
            audio_output_path,
            preprocessing_enabled: config.preprocessing_enabled,
            preprocessing_highpass_hz: config.preprocessing_highpass_hz,
            preprocessing_agc_target_rms: config.preprocessing_agc_target_rms,
            whisper_server_url: config.whisper_server_url.clone(),
            whisper_server_model: config.whisper_server_model.clone(),
            stt_alias: config.stt_alias.clone(),
            stt_postprocess: config.stt_postprocess,
            initial_audio_buffer,
            auto_end_enabled,
            auto_end_silence_ms,
            native_stt_shadow_enabled: config.native_stt_shadow_enabled,
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            model_path: PathBuf::new(),
            language: "en".to_string(),
            vad_threshold: 0.5,
            silence_to_flush_ms: 500,
            max_utterance_ms: 25000,
            diarization_enabled: false,
            diarization_model_path: None,
            speaker_similarity_threshold: 0.5,
            max_speakers: 10,
            enhancement_enabled: false,
            enhancement_model_path: None,
            biomarkers_enabled: true,
            yamnet_model_path: None,
            audio_output_path: None,
            preprocessing_enabled: true,
            preprocessing_highpass_hz: 80,
            preprocessing_agc_target_rms: 0.1,
            whisper_server_url: "http://10.241.15.154:8001".to_string(),
            whisper_server_model: "large-v3-turbo".to_string(),
            stt_alias: "medical-streaming".to_string(),
            stt_postprocess: true,
            initial_audio_buffer: None,
            auto_end_enabled: true,
            auto_end_silence_ms: 180_000, // 3 minutes default
            native_stt_shadow_enabled: false,
        }
    }
}

/// Handle to control a running pipeline
///
/// This type is `Send` but not `Sync`. It should always be stored behind a `Mutex`
/// when shared across threads (which Tauri's state management handles automatically).
pub struct PipelineHandle {
    stop_flag: Arc<AtomicBool>,
    reset_silence_flag: Arc<AtomicBool>,
    reset_biomarkers_flag: Arc<AtomicBool>,
    processor_handle: Option<std::thread::JoinHandle<()>>,
    /// Native STT shadow accumulator — shared with pipeline thread for draining at encounter boundaries
    native_stt_accumulator: Option<Arc<std::sync::Mutex<crate::native_stt_shadow::NativeSttShadowAccumulator>>>,
}

impl PipelineHandle {
    /// Request the pipeline to stop
    pub fn stop(&self) {
        info!("Requesting pipeline stop");
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Reset the silence timer (cancels auto-end countdown)
    pub fn reset_silence_timer(&self) {
        info!("Resetting silence timer");
        self.reset_silence_flag.store(true, Ordering::SeqCst);
    }

    /// Reset biomarker accumulators (triggered on encounter boundary in continuous mode)
    #[allow(dead_code)]
    pub fn reset_biomarkers(&self) {
        info!("Requesting biomarker reset");
        self.reset_biomarkers_flag.store(true, Ordering::SeqCst);
    }

    /// Get a clone of the reset biomarkers flag (for external tasks to trigger resets)
    pub fn reset_biomarkers_flag(&self) -> Arc<AtomicBool> {
        self.reset_biomarkers_flag.clone()
    }

    /// Get the native STT shadow accumulator (for draining at encounter boundaries in continuous mode)
    pub fn native_stt_accumulator(&self) -> Option<Arc<std::sync::Mutex<crate::native_stt_shadow::NativeSttShadowAccumulator>>> {
        self.native_stt_accumulator.clone()
    }

    /// Wait for the pipeline to fully stop
    pub fn join(mut self) {
        if let Some(handle) = self.processor_handle.take() {
            let _ = handle.join();
        }
    }

    /// Check if the pipeline is still running
    #[allow(dead_code)] // Useful for future monitoring features
    pub fn is_running(&self) -> bool {
        !self.stop_flag.load(Ordering::Relaxed)
    }
}

impl Drop for PipelineHandle {
    fn drop(&mut self) {
        if !self.stop_flag.load(Ordering::Relaxed) {
            self.stop_flag.store(true, Ordering::SeqCst);
        }
        if let Some(handle) = self.processor_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Start the transcription pipeline
///
/// Returns a handle to control the pipeline.
/// All audio capture and processing happens on a dedicated thread.
pub fn start_pipeline(
    config: PipelineConfig,
    message_tx: mpsc::Sender<PipelineMessage>,
) -> Result<PipelineHandle> {
    info!("Starting transcription pipeline");

    // Create stop flag
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    // Create reset silence flag (for canceling auto-end countdown)
    let reset_silence_flag = Arc::new(AtomicBool::new(false));
    let reset_silence_flag_clone = reset_silence_flag.clone();

    // Create reset biomarkers flag (for encounter boundary resets in continuous mode)
    let reset_biomarkers_flag = Arc::new(AtomicBool::new(false));
    let reset_biomarkers_flag_clone = reset_biomarkers_flag.clone();

    // Clone config for the processing thread
    let tx = message_tx;

    // Create shared native STT shadow accumulator (pre-thread so PipelineHandle can expose it)
    let native_stt_accumulator: Option<Arc<std::sync::Mutex<crate::native_stt_shadow::NativeSttShadowAccumulator>>> =
        if config.native_stt_shadow_enabled {
            Some(Arc::new(std::sync::Mutex::new(crate::native_stt_shadow::NativeSttShadowAccumulator::new())))
        } else {
            None
        };
    let accumulator_for_thread = native_stt_accumulator.clone();

    // Spawn the processing thread - everything happens on this thread
    let processor_handle = std::thread::spawn(move || {
        run_pipeline_thread(config, tx, stop_flag_clone, reset_silence_flag_clone, reset_biomarkers_flag_clone, accumulator_for_thread);
    });

    Ok(PipelineHandle {
        stop_flag,
        reset_silence_flag,
        reset_biomarkers_flag,
        processor_handle: Some(processor_handle),
        native_stt_accumulator,
    })
}

/// Run the entire pipeline on a single thread
fn run_pipeline_thread(
    config: PipelineConfig,
    tx: mpsc::Sender<PipelineMessage>,
    stop_flag: Arc<AtomicBool>,
    reset_silence_flag: Arc<AtomicBool>,
    reset_biomarkers_flag: Arc<AtomicBool>,
    shared_accumulator: Option<Arc<std::sync::Mutex<crate::native_stt_shadow::NativeSttShadowAccumulator>>>,
) {
    if let Err(e) = run_pipeline_thread_inner(&config, &tx, &stop_flag, &reset_silence_flag, &reset_biomarkers_flag, shared_accumulator.as_ref()) {
        let _ = tx.blocking_send(PipelineMessage::Error(e.to_string()));
    }
    let _ = tx.blocking_send(PipelineMessage::Stopped);
}

fn run_pipeline_thread_inner(
    config: &PipelineConfig,
    tx: &mpsc::Sender<PipelineMessage>,
    stop_flag: &Arc<AtomicBool>,
    reset_silence_flag: &Arc<AtomicBool>,
    reset_biomarkers_flag: &Arc<AtomicBool>,
    shared_accumulator: Option<&Arc<std::sync::Mutex<crate::native_stt_shadow::NativeSttShadowAccumulator>>>,
) -> Result<()> {
    info!("Pipeline thread started");
    info!("Device: {:?}", config.device_id);
    info!("Model: {:?}", config.model_path);
    info!("Language: {}", config.language);

    // Get audio device
    let device = get_device(config.device_id.as_deref())?;
    let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
    info!("Using audio device: {}", device_name);

    // Get input configuration (includes both StreamConfig and SampleFormat)
    let selected = select_input_config(&device)?;
    let sample_rate = selected.config.sample_rate.0;
    info!(
        "Audio config: {} Hz, {} channels, format {:?}",
        sample_rate, selected.config.channels, selected.sample_format
    );

    // Create ring buffer
    let capacity = calculate_ring_buffer_capacity(sample_rate);
    let ring_buffer = HeapRb::<f32>::new(capacity);
    let (producer, mut consumer) = ring_buffer.split();
    debug!("Ring buffer capacity: {} samples", capacity);

    // Create audio capture (on this thread)
    let capture = AudioCapture::new(&device, &selected.config, selected.sample_format, producer)?;

    // Start capturing
    capture.start()?;
    info!("Audio capture started");

    // Create remote Whisper client
    info!("Using remote Whisper server at {}", config.whisper_server_url);
    let whisper_client = WhisperServerClient::new(&config.whisper_server_url, &config.whisper_server_model)
        .map_err(|e| anyhow::anyhow!("Failed to create Whisper server client: {}", e))?;

    // Create native STT shadow client + accumulator + CSV logger (if enabled)
    let native_stt_client: Option<Arc<crate::native_stt::NativeSttClient>> = if config.native_stt_shadow_enabled {
        match crate::native_stt::NativeSttClient::new() {
            Ok(client) => {
                info!("Native STT shadow enabled (Apple SFSpeechRecognizer)");
                Some(Arc::new(client))
            }
            Err(e) => {
                warn!("Native STT shadow init failed, continuing without: {}", e);
                None
            }
        }
    } else {
        None
    };
    // Use shared accumulator from PipelineHandle if provided (for continuous mode external drain),
    // otherwise create a local one (session mode — drained via PipelineMessage at stop)
    let native_stt_accumulator: Option<Arc<std::sync::Mutex<crate::native_stt_shadow::NativeSttShadowAccumulator>>> =
        native_stt_client.as_ref().map(|_| {
            shared_accumulator
                .cloned()
                .unwrap_or_else(|| Arc::new(std::sync::Mutex::new(crate::native_stt_shadow::NativeSttShadowAccumulator::new())))
        });
    let native_stt_csv_logger: Option<Arc<std::sync::Mutex<crate::native_stt_shadow::NativeSttCsvLogger>>> =
        if native_stt_client.is_some() {
            match crate::native_stt_shadow::NativeSttCsvLogger::new() {
                Ok(logger) => Some(Arc::new(std::sync::Mutex::new(logger))),
                Err(e) => {
                    warn!("Native STT CSV logger init failed, continuing without: {}", e);
                    None
                }
            }
        } else {
            None
        };
    let native_stt_handles: Arc<std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));

    // Create resampler
    let mut resampler = AudioResampler::new(sample_rate)?;

    // Create audio preprocessor (DC removal, high-pass filter, AGC)
    let mut preprocessor: Option<AudioPreprocessor> = if config.preprocessing_enabled {
        match AudioPreprocessor::new(
            16000, // Whisper sample rate
            config.preprocessing_highpass_hz,
            config.preprocessing_agc_target_rms,
        ) {
            Ok(pp) => {
                info!(
                    "Audio preprocessing enabled: {}Hz high-pass, {:.2} AGC target",
                    config.preprocessing_highpass_hz, config.preprocessing_agc_target_rms
                );
                Some(pp)
            }
            Err(e) => {
                warn!("Failed to initialize audio preprocessing: {}, continuing without", e);
                None
            }
        }
    } else {
        info!("Audio preprocessing disabled");
        None
    };

    // Create VAD
    info!("Creating VAD...");
    let mut vad = VoiceActivityDetector::builder()
        .sample_rate(16000)
        .chunk_size(VAD_CHUNK_SIZE)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create VAD: {:?}", e))?;
    info!("VAD created successfully");

    // Create VAD config
    let vad_config = VadConfig::from_ms(
        config.vad_threshold,
        300, // pre-roll
        250, // min speech
        config.silence_to_flush_ms,
        config.max_utterance_ms,
    );

    // Create VAD pipeline
    let mut pipeline = VadGatedPipeline::with_config(vad_config);

    // Create diarization provider if enabled
    info!("Initializing diarization...");
    #[cfg(feature = "diarization")]
    let mut diarization: Option<DiarizationProvider> = if config.diarization_enabled {
        if let Some(ref model_path) = config.diarization_model_path {
            if model_path.exists() {
                info!("Loading diarization model from {:?}...", model_path);
                let diar_config = DiarizationConfig {
                    model_path: model_path.clone(),
                    similarity_threshold: config.speaker_similarity_threshold,
                    max_speakers: config.max_speakers,
                    n_threads: 2,
                    ..Default::default()
                };
                info!("Diarization config: similarity_threshold={}, max_speakers={}",
                      diar_config.similarity_threshold, diar_config.max_speakers);
                match DiarizationProvider::new(diar_config) {
                    Ok(mut provider) => {
                        info!("Diarization enabled with model: {:?}", model_path);

                        // Load enrolled speaker profiles
                        match SpeakerProfileManager::load() {
                            Ok(manager) => {
                                let profiles: &[SpeakerProfile] = manager.list();
                                if !profiles.is_empty() {
                                    info!("Loading {} enrolled speaker profiles...", profiles.len());
                                    provider.load_enrolled_speakers(profiles);
                                    let names = provider.enrolled_speaker_names();
                                    info!("Enrolled speakers loaded: {:?}", names);
                                } else {
                                    info!("No enrolled speaker profiles found");
                                }
                            }
                            Err(e) => {
                                warn!("Failed to load speaker profile manager: {}", e);
                            }
                        }

                        Some(provider)
                    }
                    Err(e) => {
                        warn!("Failed to initialize diarization: {}, continuing without", e);
                        None
                    }
                }
            } else {
                warn!("Diarization model not found at {:?}, continuing without", model_path);
                None
            }
        } else {
            warn!("Diarization enabled but no model path specified");
            None
        }
    } else {
        None
    };

    #[cfg(not(feature = "diarization"))]
    let diarization: Option<()> = None;

    // Build clinician name set from enrolled speaker profiles (for biomarker filtering)
    // Clinician roles: Physician, PA, RN, MA — patients and "other" are excluded
    let clinician_names: HashSet<String> = {
        #[cfg(feature = "diarization")]
        {
            use crate::speaker_profiles::{SpeakerProfileManager, SpeakerRole};
            match SpeakerProfileManager::load() {
                Ok(manager) => {
                    let names: HashSet<String> = manager.list()
                        .iter()
                        .filter(|p| matches!(p.role, SpeakerRole::Physician | SpeakerRole::Pa | SpeakerRole::Rn | SpeakerRole::Ma))
                        .map(|p| p.name.clone())
                        .collect();
                    if !names.is_empty() {
                        info!("Clinician names for biomarker filtering: {:?}", names);
                    }
                    names
                }
                Err(_) => HashSet::new(),
            }
        }
        #[cfg(not(feature = "diarization"))]
        {
            HashSet::new()
        }
    };

    // Create enhancement provider if enabled
    #[cfg(feature = "enhancement")]
    let mut enhancement: Option<EnhancementProvider> = if config.enhancement_enabled {
        if let Some(ref model_path) = config.enhancement_model_path {
            if model_path.exists() {
                let enh_config = EnhancementConfig {
                    model_path: model_path.clone(),
                    n_threads: 1,
                };
                match EnhancementProvider::new(enh_config) {
                    Ok(provider) => {
                        info!("Speech enhancement enabled with model: {:?}", model_path);
                        Some(provider)
                    }
                    Err(e) => {
                        warn!("Failed to initialize enhancement: {}, continuing without", e);
                        None
                    }
                }
            } else {
                warn!("Enhancement model not found at {:?}, continuing without", model_path);
                None
            }
        } else {
            warn!("Enhancement enabled but no model path specified");
            None
        }
    } else {
        None
    };

    #[cfg(not(feature = "enhancement"))]
    let _enhancement: Option<()> = None;

    // Create biomarker thread if enabled
    let biomarker_handle: Option<BiomarkerHandle> = if config.biomarkers_enabled {
        let bio_config = BiomarkerConfig {
            cough_detection_enabled: config.yamnet_model_path.as_ref().map(|p| p.exists()).unwrap_or(false),
            yamnet_model_path: config.yamnet_model_path.clone(),
            cough_threshold: 1.5, // yamnet_3s outputs logits - real coughs typically score 2.0-3.0+
            vitality_enabled: true,
            stability_enabled: true,
            session_metrics_enabled: true,
            n_threads: 1,
        };

        if bio_config.any_enabled() {
            info!("Starting biomarker analysis thread...");
            let handle = start_biomarker_thread(bio_config);
            info!("Biomarker thread started successfully");
            Some(handle)
        } else {
            info!("Biomarkers enabled but no analyzers configured");
            None
        }
    } else {
        None
    };

    // Create WAV writer if audio recording is enabled
    let mut wav_writer: Option<WavWriter<BufWriter<File>>> = if let Some(ref audio_path) = config.audio_output_path {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        match WavWriter::create(audio_path, spec) {
            Ok(writer) => {
                info!("Recording audio to: {:?}", audio_path);
                Some(writer)
            }
            Err(e) => {
                warn!("Failed to create WAV file: {}, continuing without recording", e);
                None
            }
        }
    } else {
        None
    };

    // Staging buffer for VAD chunks
    let mut staging_buffer: Vec<f32> = Vec::with_capacity(VAD_CHUNK_SIZE * 2);

    // Process initial audio buffer from listening mode (optimistic recording)
    // This is audio captured before the greeting check completed
    if let Some(ref initial_audio) = config.initial_audio_buffer {
        let buffer_duration_ms = initial_audio.len() as f32 / 16.0; // 16kHz = 16 samples/ms
        info!(
            "Processing initial audio buffer: {} samples ({:.1}ms)",
            initial_audio.len(),
            buffer_duration_ms
        );

        // The initial audio is already at 16kHz mono from listening mode
        // Apply preprocessing and write to WAV/biomarkers
        let mut processed_audio = initial_audio.clone();

        // Apply audio preprocessing (DC removal, high-pass filter, AGC)
        if let Some(ref mut pp) = preprocessor {
            pp.process(&mut processed_audio);
        }

        // Write to WAV file if recording is enabled
        if let Some(ref mut writer) = wav_writer {
            for &sample in &processed_audio {
                let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                if let Err(e) = writer.write_sample(sample_i16) {
                    warn!("Failed to write initial audio sample: {}", e);
                    break;
                }
            }
        }

        // Send to biomarker thread
        if let Some(ref bio_handle) = biomarker_handle {
            bio_handle.send_audio_chunk(processed_audio.clone(), 0); // Start at timestamp 0
        }

        // Add to staging buffer for VAD processing
        staging_buffer.extend_from_slice(&processed_audio);

        info!("Initial audio buffer processed and added to pipeline");
    }

    // Input buffer for resampler
    let input_frames = resampler.input_frames_next();
    let mut input_buffer = vec![0.0f32; input_frames];

    // Status tracking
    let mut last_status_time = std::time::Instant::now();
    let status_interval = Duration::from_millis(500);

    // Biomarker tracking for frontend events
    let mut last_biomarker_emit = std::time::Instant::now();
    let biomarker_emit_interval = Duration::from_millis(500); // 2Hz max
    let mut recent_coughs: VecDeque<CoughEvent> = VecDeque::with_capacity(5);
    let mut latest_session_metrics: Option<crate::biomarkers::SessionMetrics> = None;

    // Track audio capture overflows (buffer overruns)
    let mut last_overflow_count: u64 = 0;

    // Context for transcription
    let mut context = String::new();

    // Track consecutive transcription errors
    let mut consecutive_transcription_errors: u32 = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 3;

    // Track continuous silence for auto-end feature
    // This is different from VAD's per-utterance silence tracking - this tracks
    // continuous silence across utterances to detect when a session should auto-end
    let auto_end_enabled = config.auto_end_enabled && config.auto_end_silence_ms > 0;
    let mut continuous_silence_start: Option<std::time::Instant> = None;
    let auto_end_threshold = Duration::from_millis(config.auto_end_silence_ms);
    // Warning threshold is 60 seconds before auto-end (countdown for last minute)
    let warning_threshold = Duration::from_millis(config.auto_end_silence_ms.saturating_sub(60_000));
    let mut last_warning_second: Option<u64> = None;

    info!("Audio processor started, waiting for audio data...");
    if auto_end_enabled {
        info!("Auto-end enabled: session will end after {:?} of silence", auto_end_threshold);
    }

    loop {
        // Check stop flag
        if stop_flag.load(Ordering::Relaxed) {
            info!("Stop flag received, flushing pipeline...");

            // Stop capture first
            let _ = capture.stop();

            // DRAIN RING BUFFER: Process any remaining audio from the capture buffer
            // This prevents losing tail audio when the user stops recording
            let remaining = consumer.occupied_len();
            if remaining > 0 {
                debug!("Draining {} samples from ring buffer", remaining);
                let mut drain_buffer = vec![0.0f32; remaining];
                let drained = consumer.pop_slice(&mut drain_buffer);

                if drained > 0 {
                    // Process through resampler in chunks
                    for chunk in drain_buffer[..drained].chunks(input_frames) {
                        if chunk.len() == input_frames {
                            if let Ok(mut resampled) = resampler.process(chunk) {
                                // Apply audio preprocessing
                                if let Some(ref mut pp) = preprocessor {
                                    pp.process(&mut resampled);
                                }
                                // Write to WAV file
                                if let Some(ref mut writer) = wav_writer {
                                    for &sample in &resampled {
                                        let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                                        let _ = writer.write_sample(sample_i16);
                                    }
                                }
                                staging_buffer.extend_from_slice(&resampled);
                            }
                        }
                    }
                }
            }

            // DRAIN STAGING BUFFER: Process any partial VAD chunks
            // Even if < VAD_CHUNK_SIZE, we need to send this audio through VAD
            while staging_buffer.len() >= VAD_CHUNK_SIZE {
                let chunk: Vec<f32> = staging_buffer.drain(..VAD_CHUNK_SIZE).collect();
                pipeline.advance_audio_clock(VAD_CHUNK_SIZE);
                pipeline.process_chunk(&chunk, &mut vad);
            }

            // Handle any remaining partial chunk by zero-padding
            if !staging_buffer.is_empty() {
                debug!("Processing {} remaining samples with zero-padding", staging_buffer.len());
                let mut final_chunk = staging_buffer.drain(..).collect::<Vec<_>>();
                let original_len = final_chunk.len();
                final_chunk.resize(VAD_CHUNK_SIZE, 0.0); // Zero-pad to chunk size
                pipeline.advance_audio_clock(original_len); // Only advance by actual audio
                pipeline.process_chunk(&final_chunk, &mut vad);
            }

            pipeline.force_flush();

            // Transcribe any remaining utterances
            while let Some(mut utterance) = pipeline.pop_utterance() {
                // Apply speech enhancement if enabled
                #[cfg(feature = "enhancement")]
                let original_audio = if enhancement.is_some() {
                    Some(utterance.audio.clone())
                } else {
                    None
                };

                #[cfg(feature = "enhancement")]
                {
                    if let Some(ref mut enh) = enhancement {
                        if let Ok(enhanced) = enh.enhance(&utterance.audio) {
                            utterance.audio = enhanced;
                        }
                    }
                }

                // TODO: Pass context to transcription for improved accuracy
                let _context_ref = if context.is_empty() {
                    None
                } else {
                    Some(context.as_str())
                };

                // Transcribe (using enhanced audio if available)
                match transcribe_utterance(&whisper_client, &utterance, &config.language, &config.stt_alias, config.stt_postprocess, &tx) {
                    Ok(mut segment) => {
                        if !segment.text.is_empty() {
                            // Only run diarization if we have actual text
                            #[cfg(feature = "diarization")]
                            {
                                if let Some(ref mut diar) = diarization {
                                    #[cfg(feature = "enhancement")]
                                    let diar_audio = original_audio.as_ref().unwrap_or(&utterance.audio);
                                    #[cfg(not(feature = "enhancement"))]
                                    let diar_audio = &utterance.audio;

                                    match diar.identify_speaker_from_audio(
                                        diar_audio,
                                        utterance.start_ms,
                                        utterance.end_ms,
                                    ) {
                                        Ok((id, conf)) => {
                                            segment.speaker_id = Some(id);
                                            segment.speaker_confidence = Some(conf);
                                        }
                                        Err(e) => {
                                            debug!("Diarization failed for utterance: {}", e);
                                        }
                                    }
                                }
                            }

                            // Fork to native STT shadow (drain loop)
                            if let Some(ref client) = native_stt_client {
                                let client = client.clone();
                                let Some(accumulator) = native_stt_accumulator.clone() else {
                                    warn!("Native STT client present but accumulator missing — skipping shadow");
                                    continue;
                                };
                                let csv_logger = native_stt_csv_logger.clone();
                                #[cfg(feature = "enhancement")]
                                let native_audio = original_audio.clone().unwrap_or_else(|| utterance.audio.clone());
                                #[cfg(not(feature = "enhancement"))]
                                let native_audio = utterance.audio.clone();
                                let primary_text = segment.text.clone();
                                let speaker_id = segment.speaker_id.clone();
                                let utt_id = utterance.id;
                                let start_ms = utterance.start_ms;
                                let end_ms = utterance.end_ms;

                                let handle = std::thread::spawn(move || {
                                    let native_start = std::time::Instant::now();
                                    match client.transcribe_blocking(&native_audio, 16000) {
                                        Ok(native_text) => {
                                            let native_latency_ms = native_start.elapsed().as_millis() as u64;
                                            let seg = crate::native_stt_shadow::NativeSttSegment {
                                                utterance_id: utt_id,
                                                start_ms, end_ms,
                                                native_text, primary_text, speaker_id,
                                                native_latency_ms, primary_latency_ms: 0,
                                            };
                                            if let Some(ref logger) = csv_logger {
                                                if let Ok(mut l) = logger.lock() { l.write_segment(&seg); }
                                            }
                                            if let Ok(mut acc) = accumulator.lock() { acc.push(seg); }
                                        }
                                        Err(e) => {
                                            tracing::warn!("Native STT shadow failed (drain): {}", e);
                                        }
                                    }
                                });
                                if let Ok(mut handles) = native_stt_handles.lock() {
                                    handles.push(handle);
                                }
                            }

                            context.push(' ');
                            context.push_str(&segment.text);
                            if tx.blocking_send(PipelineMessage::Segment(segment)).is_err() {
                                break;
                            }
                            // Reset consecutive error counter on success
                            consecutive_transcription_errors = 0;
                        }
                    }
                    Err(e) => {
                        error!("Transcription error: {}", e);
                        consecutive_transcription_errors += 1;
                        if consecutive_transcription_errors >= MAX_CONSECUTIVE_ERRORS {
                            let _ = tx.blocking_send(PipelineMessage::Error(format!(
                                "Transcription service unavailable after {} attempts: {}",
                                consecutive_transcription_errors, e
                            )));
                        }
                    }
                }
            }

            // Finalize WAV file if recording
            if let Some(writer) = wav_writer.take() {
                match writer.finalize() {
                    Ok(_) => info!("Audio recording saved successfully"),
                    Err(e) => warn!("Failed to finalize WAV file: {}", e),
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
                let _ = tx.blocking_send(PipelineMessage::Status {
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

        // Check for audio capture overflows (buffer overruns) and send to biomarker thread
        let current_overflow = capture.overflow_count();
        if current_overflow > last_overflow_count {
            let new_overflows = current_overflow - last_overflow_count;
            warn!("Audio buffer overflow detected: {} new overruns", new_overflows);
            if let Some(ref bio_handle) = biomarker_handle {
                for _ in 0..new_overflows {
                    bio_handle.send_dropout();
                }
            }
            last_overflow_count = current_overflow;
        }

        // Resample to 16kHz
        let mut resampled = resampler.process(&input_buffer)?;

        // Apply audio preprocessing (DC removal, high-pass filter, AGC)
        if let Some(ref mut pp) = preprocessor {
            pp.process(&mut resampled);
        }

        // Write to WAV file if recording is enabled
        if let Some(ref mut writer) = wav_writer {
            for &sample in &resampled {
                // Convert f32 [-1.0, 1.0] to i16
                let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                if let Err(e) = writer.write_sample(sample_i16) {
                    warn!("Failed to write audio sample: {}", e);
                    break;
                }
            }
        }

        // CLONE POINT 1: Send resampled audio to biomarker thread (for YAMNet cough detection)
        // YAMNet needs ALL audio, including silence, to detect coughs
        if let Some(ref bio_handle) = biomarker_handle {
            bio_handle.send_audio_chunk(resampled.clone(), pipeline.audio_clock_ms());
        }

        // Accumulate into staging buffer
        staging_buffer.extend_from_slice(&resampled);

        // Process complete VAD chunks
        while staging_buffer.len() >= VAD_CHUNK_SIZE {
            let chunk: Vec<f32> = staging_buffer.drain(..VAD_CHUNK_SIZE).collect();

            // Advance audio clock by chunk size (in 16kHz samples)
            pipeline.advance_audio_clock(VAD_CHUNK_SIZE);

            // VAD + accumulation (returns whether speech was detected)
            let is_speech = pipeline.process_chunk(&chunk, &mut vad);

            // Send audio with VAD state to biomarker thread for quality analysis
            if let Some(ref bio_handle) = biomarker_handle {
                bio_handle.send_audio_chunk_with_vad(chunk, pipeline.audio_clock_ms(), is_speech);
            }

            // Track continuous silence for auto-end feature
            if auto_end_enabled {
                // Check if user cancelled the auto-end countdown
                if reset_silence_flag.swap(false, Ordering::SeqCst) {
                    info!("User cancelled auto-end countdown, resetting silence timer");
                    if continuous_silence_start.is_some() {
                        // Emit a "cancelled" warning with 0 remaining to clear UI countdown
                        let _ = tx.blocking_send(PipelineMessage::SilenceWarning {
                            silence_ms: 0,
                            remaining_ms: 0,
                        });
                    }
                    continuous_silence_start = None;
                    last_warning_second = None;
                }

                if is_speech {
                    // Speech detected - reset silence timer and warning state
                    if continuous_silence_start.is_some() {
                        debug!("Speech detected, resetting auto-end silence timer");
                        // Emit a "cancelled" warning with 0 remaining to clear UI countdown
                        let _ = tx.blocking_send(PipelineMessage::SilenceWarning {
                            silence_ms: 0,
                            remaining_ms: 0,
                        });
                    }
                    continuous_silence_start = None;
                    last_warning_second = None;
                } else {
                    // No speech - start or continue silence timer
                    if continuous_silence_start.is_none() {
                        continuous_silence_start = Some(std::time::Instant::now());
                    }

                    // Check silence duration and emit warnings/auto-end
                    if let Some(start) = continuous_silence_start {
                        let silence_duration = start.elapsed();
                        let silence_ms = silence_duration.as_millis() as u64;

                        if silence_duration >= auto_end_threshold {
                            // Auto-end threshold reached
                            info!(
                                "Auto-end triggered: {} seconds of continuous silence",
                                silence_ms / 1000
                            );
                            let _ = tx.blocking_send(PipelineMessage::AutoEndSilence {
                                silence_duration_ms: silence_ms,
                            });
                            // Reset to prevent duplicate messages
                            continuous_silence_start = None;
                            // Set stop flag to trigger graceful shutdown
                            stop_flag.store(true, Ordering::SeqCst);
                        } else if silence_duration >= warning_threshold {
                            // Past warning threshold - emit countdown warnings every second
                            let current_second = silence_ms / 1000;
                            let remaining_ms = config.auto_end_silence_ms.saturating_sub(silence_ms);

                            if last_warning_second != Some(current_second) {
                                last_warning_second = Some(current_second);
                                debug!(
                                    "Silence warning: {}s elapsed, {}s remaining",
                                    current_second,
                                    remaining_ms / 1000
                                );
                                let _ = tx.blocking_send(PipelineMessage::SilenceWarning {
                                    silence_ms,
                                    remaining_ms,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Transcribe any ready utterances
        while let Some(mut utterance) = pipeline.pop_utterance() {
            debug!(
                "Transcribing utterance: {}ms - {}ms",
                utterance.start_ms, utterance.end_ms
            );

            // CLONE POINT 2: Send original audio to biomarker thread (for vitality/stability)
            // This must happen BEFORE enhancement to get unmodified voice characteristics
            if let Some(ref bio_handle) = biomarker_handle {
                bio_handle.send_utterance(
                    utterance.id,
                    utterance.audio.clone(),
                    utterance.start_ms,
                    utterance.end_ms,
                );
            }

            // Apply speech enhancement if enabled
            #[cfg(feature = "enhancement")]
            let original_audio = if enhancement.is_some() {
                Some(utterance.audio.clone())
            } else {
                None
            };

            #[cfg(feature = "enhancement")]
            {
                if let Some(ref mut enh) = enhancement {
                    match enh.enhance(&utterance.audio) {
                        Ok(enhanced) => {
                            info!("GTCRN enhanced audio: {} -> {} samples", utterance.audio.len(), enhanced.len());
                            utterance.audio = enhanced;
                        }
                        Err(e) => {
                            warn!("Enhancement failed, using original audio: {}", e);
                        }
                    }
                }
            }

            // TODO: Pass context to transcription for improved accuracy
            let _context_ref = if context.is_empty() {
                None
            } else {
                Some(context.as_str())
            };

            // Transcribe (using enhanced audio if available)
            match transcribe_utterance(&whisper_client, &utterance, &config.language, &config.stt_alias, config.stt_postprocess, &tx) {
                Ok(mut segment) => {
                    if !segment.text.is_empty() {
                        // Use original audio for diarization (speaker fingerprints)
                        #[cfg(feature = "diarization")]
                        {
                            if let Some(ref mut diar) = diarization {
                                // Use original audio if we enhanced, otherwise use utterance audio
                                #[cfg(feature = "enhancement")]
                                let diar_audio = original_audio.as_ref().unwrap_or(&utterance.audio);
                                #[cfg(not(feature = "enhancement"))]
                                let diar_audio = &utterance.audio;

                                info!("Running diarization on utterance with {} samples", diar_audio.len());
                                match diar.identify_speaker_from_audio(
                                    diar_audio,
                                    utterance.start_ms,
                                    utterance.end_ms,
                                ) {
                                    Ok((id, conf)) => {
                                        info!("Diarization result: {} ({:.0}% confidence)", id, conf * 100.0);
                                        segment.speaker_id = Some(id);
                                        segment.speaker_confidence = Some(conf);
                                    }
                                    Err(e) => {
                                        warn!("Diarization failed for utterance: {}", e);
                                    }
                                }
                            }
                        }

                        // Try to get biomarker results for this segment
                        if let Some(ref bio_handle) = biomarker_handle {
                            // Send segment info for session metrics
                            bio_handle.send_segment_info(
                                segment.speaker_id.clone(),
                                segment.start_ms,
                                segment.end_ms,
                            );

                            // Try to receive any available biomarker outputs (non-blocking)
                            while let Some(output) = bio_handle.try_recv() {
                                match output {
                                    BiomarkerOutput::VocalBiomarkers(bio) => {
                                        // Attach biomarkers to segment if they match
                                        if bio.utterance_id == utterance.id {
                                            debug!(
                                                "Attaching biomarkers: vitality={:?} stability={:?}",
                                                bio.vitality, bio.stability
                                            );
                                            segment.vocal_biomarkers = Some(bio);
                                        }
                                    }
                                    BiomarkerOutput::CoughEvent(event) => {
                                        debug!(
                                            "Cough detected: {} at {}ms (conf: {:.2})",
                                            event.label, event.timestamp_ms, event.confidence
                                        );
                                        // Buffer recent coughs (last 5)
                                        recent_coughs.push_back(event);
                                        if recent_coughs.len() > 5 {
                                            recent_coughs.pop_front();
                                        }
                                    }
                                    BiomarkerOutput::SessionMetrics(metrics) => {
                                        debug!(
                                            "Session metrics: {} coughs, {} turns",
                                            metrics.cough_count, metrics.turn_count
                                        );
                                        // Store latest metrics for throttled emission
                                        latest_session_metrics = Some(metrics);
                                    }
                                    BiomarkerOutput::AudioQuality(snapshot) => {
                                        debug!(
                                            "Audio quality: RMS={:.1}dB SNR={:.1}dB clips={}",
                                            snapshot.rms_db, snapshot.snr_db, snapshot.clipped_samples
                                        );
                                        // Emit audio quality update to frontend
                                        let _ = tx.blocking_send(PipelineMessage::AudioQuality(snapshot));
                                    }
                                }
                            }

                            // Check biomarker reset flag (encounter boundary in continuous mode)
                            if reset_biomarkers_flag.swap(false, Ordering::SeqCst) {
                                info!("Biomarker reset flag detected — resetting accumulators");
                                bio_handle.send_reset();
                                recent_coughs.clear();
                                latest_session_metrics = None;
                            }

                            // Emit biomarker update to frontend (throttled to 2Hz)
                            if last_biomarker_emit.elapsed() >= biomarker_emit_interval {
                                if let Some(ref metrics) = latest_session_metrics {
                                    let coughs_vec: Vec<CoughEvent> = recent_coughs.iter().cloned().collect();
                                    let mut update = BiomarkerUpdate::from_metrics(metrics, &coughs_vec);
                                    // Annotate clinician status on speaker metrics
                                    for speaker in &mut update.speaker_metrics {
                                        speaker.is_clinician = clinician_names.contains(&speaker.speaker_id);
                                    }
                                    let _ = tx.blocking_send(PipelineMessage::Biomarker(update));
                                    last_biomarker_emit = std::time::Instant::now();
                                }
                            }
                        }

                        // Fork utterance to native STT shadow (if enabled)
                        if let Some(ref client) = native_stt_client {
                            let client = client.clone();
                            let Some(accumulator) = native_stt_accumulator.clone() else {
                                warn!("Native STT client present but accumulator missing — skipping shadow");
                                break;
                            };
                            let csv_logger = native_stt_csv_logger.clone();
                            // Use pre-enhancement audio (same as what STT Router receives)
                            #[cfg(feature = "enhancement")]
                            let native_audio = original_audio.clone().unwrap_or_else(|| utterance.audio.clone());
                            #[cfg(not(feature = "enhancement"))]
                            let native_audio = utterance.audio.clone();
                            let primary_text = segment.text.clone();
                            let speaker_id = segment.speaker_id.clone();
                            let utt_id = utterance.id;
                            let start_ms = utterance.start_ms;
                            let end_ms = utterance.end_ms;
                            // Primary latency not tracked here (would need timing around transcribe_utterance)
                            let primary_latency_ms = 0u64;

                            let handle = std::thread::spawn(move || {
                                let native_start = std::time::Instant::now();
                                match client.transcribe_blocking(&native_audio, 16000) {
                                    Ok(native_text) => {
                                        let native_latency_ms = native_start.elapsed().as_millis() as u64;
                                        let seg = crate::native_stt_shadow::NativeSttSegment {
                                            utterance_id: utt_id,
                                            start_ms,
                                            end_ms,
                                            native_text,
                                            primary_text,
                                            speaker_id,
                                            native_latency_ms,
                                            primary_latency_ms,
                                        };
                                        // Log to CSV
                                        if let Some(ref logger) = csv_logger {
                                            if let Ok(mut l) = logger.lock() {
                                                l.write_segment(&seg);
                                            }
                                        }
                                        // Push to accumulator
                                        if let Ok(mut acc) = accumulator.lock() {
                                            acc.push(seg);
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Native STT shadow failed for utterance {}: {}", utt_id, e);
                                    }
                                }
                            });
                            if let Ok(mut handles) = native_stt_handles.lock() {
                                handles.push(handle);
                            }
                        }

                        // Log segment metadata only - no transcript text (PHI)
                        info!("Sending segment: {} words ({}ms - {}ms)", segment.text.split_whitespace().count(), segment.start_ms, segment.end_ms);

                        // Update context
                        context.push(' ');
                        context.push_str(&segment.text);

                        // Keep context reasonable size
                        if context.len() > 1000 {
                            let words: Vec<&str> = context.split_whitespace().collect();
                            context = words
                                .into_iter()
                                .rev()
                                .take(100)
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect::<Vec<_>>()
                                .join(" ");
                        }

                        if tx.blocking_send(PipelineMessage::Segment(segment)).is_err() {
                            warn!("Failed to send segment, receiver dropped");
                            let _ = capture.stop();
                            return Ok(());
                        }
                        // Reset consecutive error counter on success
                        consecutive_transcription_errors = 0;
                    } else {
                        info!("Skipping empty segment");
                    }
                }
                Err(e) => {
                    error!("Transcription error: {}", e);
                    consecutive_transcription_errors += 1;
                    if consecutive_transcription_errors >= MAX_CONSECUTIVE_ERRORS {
                        let _ = tx.blocking_send(PipelineMessage::Error(format!(
                            "Transcription service unavailable after {} attempts: {}",
                            consecutive_transcription_errors, e
                        )));
                    }
                }
            }
        }

        // Send status updates periodically
        if last_status_time.elapsed() >= status_interval {
            let _ = tx.blocking_send(PipelineMessage::Status {
                audio_clock_ms: pipeline.audio_clock_ms(),
                pending_count: pipeline.pending_count(),
                is_speech_active: pipeline.is_speech_active(),
            });
            last_status_time = std::time::Instant::now();
        }
    }

    // Stop biomarker thread first (before dropping ONNX providers)
    if let Some(bio_handle) = biomarker_handle {
        info!("Stopping biomarker thread");
        bio_handle.stop();
        bio_handle.join();
        info!("Biomarker thread stopped");
    }

    // Explicitly drop heavy resources in the correct order
    // to avoid ONNX Runtime mutex issues during shutdown
    #[cfg(feature = "diarization")]
    drop(diarization);

    #[cfg(feature = "enhancement")]
    drop(enhancement);

    drop(whisper_client);

    // Small delay to let ONNX/Whisper C++ destructors complete
    std::thread::sleep(Duration::from_millis(50));

    // Join all in-flight native STT threads before draining accumulator
    {
        let handles_to_join: Vec<std::thread::JoinHandle<()>> = native_stt_handles
            .lock()
            .map(|mut h| std::mem::take(&mut *h))
            .unwrap_or_default();
        if !handles_to_join.is_empty() {
            info!("Waiting for {} native STT shadow threads to complete", handles_to_join.len());
            for handle in handles_to_join {
                let _ = handle.join();
            }
        }
    }

    // Drain native STT shadow accumulator and send transcript before Stopped
    if let Some(ref accumulator) = native_stt_accumulator {
        if let Ok(mut acc) = accumulator.lock() {
            if !acc.is_empty() {
                let (transcript, _segments) = acc.drain_all();
                if !transcript.is_empty() {
                    info!("Sending native STT shadow transcript ({} chars)", transcript.len());
                    let _ = tx.blocking_send(PipelineMessage::NativeSttShadowTranscript { transcript });
                }
            }
        }
    }

    info!("Audio processor stopped");
    Ok(())
}
