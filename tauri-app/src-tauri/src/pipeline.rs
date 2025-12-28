use anyhow::Result;
use cpal::traits::DeviceTrait;
use ringbuf::traits::{Consumer as ConsumerTrait, Observer, Split};
use ringbuf::HeapRb;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use voice_activity_detector::VoiceActivityDetector;

use crate::audio::{calculate_ring_buffer_capacity, get_device, select_input_config, AudioCapture, AudioResampler};
use crate::transcription::{Segment, WhisperProvider};
use crate::vad::{VadConfig, VadGatedPipeline};

#[cfg(feature = "diarization")]
use crate::diarization::{DiarizationConfig, DiarizationProvider};

#[cfg(feature = "enhancement")]
use crate::enhancement::{EnhancementConfig, EnhancementProvider};

#[cfg(feature = "emotion")]
use crate::emotion::{EmotionConfig, EmotionProvider};

use crate::biomarkers::{BiomarkerConfig, BiomarkerHandle, BiomarkerOutput, start_biomarker_thread};

/// VAD chunk size at 16kHz
const VAD_CHUNK_SIZE: usize = 512;

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
    // Emotion detection settings
    #[allow(dead_code)]
    pub emotion_enabled: bool,
    #[allow(dead_code)]
    pub emotion_model_path: Option<PathBuf>,
    // Biomarker analysis settings
    pub biomarkers_enabled: bool,
    pub yamnet_model_path: Option<PathBuf>,
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
            emotion_enabled: false,
            emotion_model_path: None,
            biomarkers_enabled: true,
            yamnet_model_path: None,
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

    // Load Whisper model
    info!("Loading Whisper model...");
    let whisper = WhisperProvider::new(&config.model_path, &config.language, config.n_threads)?;
    info!("Whisper model loaded");

    // Create resampler
    let mut resampler = AudioResampler::new(sample_rate)?;

    // Create VAD
    let mut vad = VoiceActivityDetector::builder()
        .sample_rate(16000)
        .chunk_size(VAD_CHUNK_SIZE)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create VAD: {:?}", e))?;

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
    #[cfg(feature = "diarization")]
    let mut diarization: Option<DiarizationProvider> = if config.diarization_enabled {
        if let Some(ref model_path) = config.diarization_model_path {
            if model_path.exists() {
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

    // Create emotion detection provider if enabled
    #[cfg(feature = "emotion")]
    let mut emotion: Option<EmotionProvider> = if config.emotion_enabled {
        if let Some(ref model_path) = config.emotion_model_path {
            if model_path.exists() {
                let emo_config = EmotionConfig {
                    model_path: model_path.clone(),
                    n_threads: 1,
                    ..Default::default()
                };
                match EmotionProvider::new(emo_config) {
                    Ok(provider) => {
                        info!("Emotion detection enabled with model: {:?}", model_path);
                        Some(provider)
                    }
                    Err(e) => {
                        warn!("Failed to initialize emotion detection: {}, continuing without", e);
                        None
                    }
                }
            } else {
                warn!("Emotion model not found at {:?}, continuing without", model_path);
                None
            }
        } else {
            warn!("Emotion detection enabled but no model path specified");
            None
        }
    } else {
        None
    };

    #[cfg(not(feature = "emotion"))]
    let _emotion: Option<()> = None;

    // Create biomarker thread if enabled
    let biomarker_handle: Option<BiomarkerHandle> = if config.biomarkers_enabled {
        let bio_config = BiomarkerConfig {
            cough_detection_enabled: config.yamnet_model_path.as_ref().map(|p| p.exists()).unwrap_or(false),
            yamnet_model_path: config.yamnet_model_path.clone(),
            cough_threshold: 0.5,
            vitality_enabled: true,
            stability_enabled: true,
            session_metrics_enabled: true,
            n_threads: 1,
        };

        if bio_config.any_enabled() {
            info!("Starting biomarker analysis thread");
            Some(start_biomarker_thread(bio_config))
        } else {
            info!("Biomarkers enabled but no analyzers configured");
            None
        }
    } else {
        None
    };

    // Staging buffer for VAD chunks
    let mut staging_buffer: Vec<f32> = Vec::with_capacity(VAD_CHUNK_SIZE * 2);

    // Input buffer for resampler
    let input_frames = resampler.input_frames_next();
    let mut input_buffer = vec![0.0f32; input_frames];

    // Status tracking
    let mut last_status_time = std::time::Instant::now();
    let status_interval = Duration::from_millis(500);

    // Context for transcription
    let mut context = String::new();

    info!("Audio processor started, waiting for audio data...");

    loop {
        // Check stop flag
        if stop_flag.load(Ordering::Relaxed) {
            info!("Stop flag received, flushing pipeline...");

            // Stop capture first
            let _ = capture.stop();

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
                match whisper.transcribe(&utterance, context_ref) {
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

                            // Detect emotion if enabled
                            #[cfg(feature = "emotion")]
                            {
                                if let Some(ref mut emo) = emotion {
                                    #[cfg(feature = "enhancement")]
                                    let emo_audio = original_audio.as_ref().unwrap_or(&utterance.audio);
                                    #[cfg(not(feature = "enhancement"))]
                                    let emo_audio = &utterance.audio;

                                    if let Ok(result) = emo.detect(emo_audio) {
                                        segment.emotion = Some(result);
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

        // Resample to 16kHz
        let resampled = resampler.process(&input_buffer)?;

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

            // VAD + accumulation
            pipeline.process_chunk(&chunk, &mut vad);
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
            match whisper.transcribe(&utterance, context_ref) {
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

                        // Detect emotion if enabled
                        #[cfg(feature = "emotion")]
                        {
                            if let Some(ref mut emo) = emotion {
                                // Use original audio for emotion detection
                                #[cfg(feature = "enhancement")]
                                let emo_audio = original_audio.as_ref().unwrap_or(&utterance.audio);
                                #[cfg(not(feature = "enhancement"))]
                                let emo_audio = &utterance.audio;

                                match emo.detect(emo_audio) {
                                    Ok(result) => {
                                        debug!(
                                            "Emotion detected: {} (A:{:.2} D:{:.2} V:{:.2})",
                                            result.label(),
                                            result.arousal,
                                            result.dominance,
                                            result.valence
                                        );
                                        segment.emotion = Some(result);
                                    }
                                    Err(e) => {
                                        debug!("Emotion detection failed: {}", e);
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
                                        // TODO: Emit cough event to frontend
                                    }
                                    BiomarkerOutput::SessionMetrics(metrics) => {
                                        debug!(
                                            "Session metrics: {} coughs, {} turns",
                                            metrics.cough_count, metrics.turn_count
                                        );
                                        // TODO: Emit session metrics to frontend
                                    }
                                }
                            }
                        }

                        info!("Sending segment: '{}' ({}ms - {}ms)", segment.text, segment.start_ms, segment.end_ms);

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

    #[cfg(feature = "emotion")]
    drop(emotion);

    drop(whisper);

    // Small delay to let ONNX/Whisper C++ destructors complete
    std::thread::sleep(Duration::from_millis(50));

    info!("Audio processor stopped");
    Ok(())
}
