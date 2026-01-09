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
use crate::transcription::{Segment, Utterance, WhisperProvider};
use crate::vad::{VadConfig, VadGatedPipeline};
use crate::whisper_server::WhisperServerClient;

#[cfg(feature = "diarization")]
use crate::diarization::{DiarizationConfig, DiarizationProvider};

#[cfg(feature = "enhancement")]
use crate::enhancement::{EnhancementConfig, EnhancementProvider};

use crate::biomarkers::{AudioQualitySnapshot, BiomarkerConfig, BiomarkerHandle, BiomarkerOutput, BiomarkerUpdate, CoughEvent, start_biomarker_thread};
use std::collections::VecDeque;

/// VAD chunk size at 16kHz
const VAD_CHUNK_SIZE: usize = 512;

/// Transcription provider - remote Whisper server only
/// Note: Local variant kept for potential future use but marked dead_code
enum TranscriptionProvider {
    /// Local Whisper model via whisper-rs (not currently used)
    #[allow(dead_code)]
    Local(WhisperProvider),
    /// Remote Whisper server (faster-whisper-server/Speaches)
    Remote(WhisperServerClient),
}

impl TranscriptionProvider {
    /// Transcribe an utterance and return a segment
    fn transcribe(&self, utterance: &Utterance, context: Option<&str>, language: &str) -> Result<Segment, String> {
        match self {
            TranscriptionProvider::Local(whisper) => {
                whisper.transcribe(utterance, context).map_err(|e| e.to_string())
            }
            TranscriptionProvider::Remote(client) => {
                // Use blocking transcribe for remote server
                let text = client.transcribe_blocking(&utterance.audio, language)?;
                Ok(Segment::new(
                    utterance.start_ms,
                    utterance.end_ms,
                    text,
                ))
            }
        }
    }
}

/// Message from the transcription pipeline to the session controller
#[derive(Debug)]
#[allow(dead_code)] // Fields used for debugging and future UI features
pub enum PipelineMessage {
    /// New segment transcribed
    Segment(Segment),
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
    pub n_threads: i32,
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
    // Whisper server settings (for remote transcription)
    pub whisper_mode: String,
    pub whisper_server_url: String,
    pub whisper_server_model: String,
    // Initial audio buffer from listening mode (optimistic recording)
    // This buffer contains audio captured before the greeting check completed
    // and should be prepended to the recording at startup
    pub initial_audio_buffer: Option<Vec<f32>>,
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
            n_threads: 4,
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
            whisper_mode: "remote".to_string(),  // Always use remote server
            whisper_server_url: "http://172.16.100.45:8001".to_string(),
            whisper_server_model: "large-v3-turbo".to_string(),
            initial_audio_buffer: None,
        }
    }
}

/// Handle to control a running pipeline
///
/// This type is `Send` but not `Sync`. It should always be stored behind a `Mutex`
/// when shared across threads (which Tauri's state management handles automatically).
pub struct PipelineHandle {
    stop_flag: Arc<AtomicBool>,
    processor_handle: Option<std::thread::JoinHandle<()>>,
}

impl PipelineHandle {
    /// Request the pipeline to stop
    pub fn stop(&self) {
        info!("Requesting pipeline stop");
        self.stop_flag.store(true, Ordering::SeqCst);
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

    // Clone config for the processing thread
    let tx = message_tx;

    // Spawn the processing thread - everything happens on this thread
    let processor_handle = std::thread::spawn(move || {
        run_pipeline_thread(config, tx, stop_flag_clone);
    });

    Ok(PipelineHandle {
        stop_flag,
        processor_handle: Some(processor_handle),
    })
}

/// Run the entire pipeline on a single thread
fn run_pipeline_thread(
    config: PipelineConfig,
    tx: mpsc::Sender<PipelineMessage>,
    stop_flag: Arc<AtomicBool>,
) {
    if let Err(e) = run_pipeline_thread_inner(&config, &tx, &stop_flag) {
        let _ = tx.blocking_send(PipelineMessage::Error(e.to_string()));
    }
    let _ = tx.blocking_send(PipelineMessage::Stopped);
}

fn run_pipeline_thread_inner(
    config: &PipelineConfig,
    tx: &mpsc::Sender<PipelineMessage>,
    stop_flag: &Arc<AtomicBool>,
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

    // Create transcription provider (always remote)
    info!("Using remote Whisper server at {}", config.whisper_server_url);
    let client = WhisperServerClient::new(&config.whisper_server_url, &config.whisper_server_model)
        .map_err(|e| anyhow::anyhow!("Failed to create Whisper server client: {}", e))?;
    let provider = TranscriptionProvider::Remote(client);

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
                    Ok(provider) => {
                        info!("Diarization enabled with model: {:?}", model_path);
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

    info!("Audio processor started, waiting for audio data...");

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

                let context_ref = if context.is_empty() {
                    None
                } else {
                    Some(context.as_str())
                };

                // Transcribe (using enhanced audio if available)
                match provider.transcribe(&utterance, context_ref, &config.language) {
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

                            context.push(' ');
                            context.push_str(&segment.text);
                            if tx.blocking_send(PipelineMessage::Segment(segment)).is_err() {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Transcription error: {}", e);
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

            let context_ref = if context.is_empty() {
                None
            } else {
                Some(context.as_str())
            };

            // Transcribe (using enhanced audio if available)
            match provider.transcribe(&utterance, context_ref, &config.language) {
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

                            // Emit biomarker update to frontend (throttled to 2Hz)
                            if last_biomarker_emit.elapsed() >= biomarker_emit_interval {
                                if let Some(ref metrics) = latest_session_metrics {
                                    let coughs_vec: Vec<CoughEvent> = recent_coughs.iter().cloned().collect();
                                    let update = BiomarkerUpdate::from_metrics(metrics, &coughs_vec);
                                    let _ = tx.blocking_send(PipelineMessage::Biomarker(update));
                                    last_biomarker_emit = std::time::Instant::now();
                                }
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
                    } else {
                        info!("Skipping empty segment");
                    }
                }
                Err(e) => {
                    error!("Transcription error: {}", e);
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

    drop(provider);

    // Small delay to let ONNX/Whisper C++ destructors complete
    std::thread::sleep(Duration::from_millis(50));

    info!("Audio processor stopped");
    Ok(())
}
