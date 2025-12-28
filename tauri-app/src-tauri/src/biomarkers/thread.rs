//! Biomarker processing thread
//!
//! Runs in parallel with the main transcription pipeline, processing:
//! - Continuous audio for YAMNet cough detection
//! - Utterances for vitality/stability analysis
//! - Segment info for session metrics

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use tracing::{debug, info, warn};

use super::config::BiomarkerConfig;
use super::session_metrics::SessionAggregator;
use super::voice_metrics::{calculate_stability, calculate_vitality};
use super::{BiomarkerInput, BiomarkerOutput, VocalBiomarkers};

#[cfg(feature = "biomarkers")]
use super::yamnet::YamnetProvider;

/// Handle to control the biomarker thread
pub struct BiomarkerHandle {
    /// Channel to send inputs to the biomarker thread
    input_tx: Sender<BiomarkerInput>,
    /// Channel to receive outputs from the biomarker thread
    output_rx: Receiver<BiomarkerOutput>,
    /// Stop flag
    stop_flag: Arc<AtomicBool>,
    /// Thread handle
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl BiomarkerHandle {
    /// Send an audio chunk for continuous analysis (YAMNet)
    pub fn send_audio_chunk(&self, samples: Vec<f32>, timestamp_ms: u64) {
        let _ = self.input_tx.send(BiomarkerInput::AudioChunk {
            samples,
            timestamp_ms,
        });
    }

    /// Send an utterance for vitality/stability analysis
    pub fn send_utterance(
        &self,
        id: uuid::Uuid,
        samples: Vec<f32>,
        start_ms: u64,
        end_ms: u64,
    ) {
        let _ = self.input_tx.send(BiomarkerInput::Utterance {
            id,
            samples,
            start_ms,
            end_ms,
        });
    }

    /// Send segment info for session metrics
    pub fn send_segment_info(&self, speaker_id: Option<String>, start_ms: u64, end_ms: u64) {
        let _ = self.input_tx.send(BiomarkerInput::SegmentInfo {
            speaker_id,
            start_ms,
            end_ms,
        });
    }

    /// Try to receive a biomarker output (non-blocking)
    pub fn try_recv(&self) -> Option<BiomarkerOutput> {
        self.output_rx.try_recv().ok()
    }

    /// Request the thread to stop
    pub fn stop(&self) {
        info!("Requesting biomarker thread stop");
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.input_tx.send(BiomarkerInput::Shutdown);
    }

    /// Wait for the thread to finish
    pub fn join(mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Start the biomarker processing thread
pub fn start_biomarker_thread(config: BiomarkerConfig) -> BiomarkerHandle {
    let (input_tx, input_rx) = mpsc::channel();
    let (output_tx, output_rx) = mpsc::channel();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    let thread_handle = thread::spawn(move || {
        run_biomarker_thread(config, input_rx, output_tx, stop_flag_clone);
    });

    BiomarkerHandle {
        input_tx,
        output_rx,
        stop_flag,
        thread_handle: Some(thread_handle),
    }
}

fn run_biomarker_thread(
    config: BiomarkerConfig,
    input_rx: Receiver<BiomarkerInput>,
    output_tx: Sender<BiomarkerOutput>,
    stop_flag: Arc<AtomicBool>,
) {
    info!("Biomarker thread started");
    info!(
        "  Cough detection: {} (YAMNet ready: {})",
        config.cough_detection_enabled,
        config.yamnet_ready()
    );
    info!("  Vitality: {}", config.vitality_enabled);
    info!("  Stability: {}", config.stability_enabled);
    info!("  Session metrics: {}", config.session_metrics_enabled);

    // Initialize YAMNet provider if enabled and model available
    #[cfg(feature = "biomarkers")]
    let mut yamnet: Option<YamnetProvider> = if config.yamnet_ready() {
        match YamnetProvider::new(config.yamnet_model_path.as_ref().unwrap(), config.n_threads) {
            Ok(provider) => {
                info!("YAMNet provider initialized");
                Some(provider)
            }
            Err(e) => {
                warn!("Failed to initialize YAMNet: {}", e);
                None
            }
        }
    } else {
        None
    };

    #[cfg(not(feature = "biomarkers"))]
    let yamnet: Option<()> = None;

    // Session metrics aggregator
    let mut session = SessionAggregator::new();

    // Vitality/stability accumulators for session averages
    let mut vitality_values: Vec<f32> = Vec::new();
    let mut stability_values: Vec<f32> = Vec::new();

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // Block waiting for input with timeout to check stop flag
        let input = match input_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(input) => input,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                info!("Biomarker input channel disconnected");
                break;
            }
        };

        match input {
            BiomarkerInput::AudioChunk {
                samples,
                timestamp_ms,
            } => {
                // YAMNet cough detection on continuous audio
                #[cfg(feature = "biomarkers")]
                if let Some(ref mut yam) = yamnet {
                    match yam.process_chunk(&samples, timestamp_ms, config.cough_threshold) {
                        Ok(events) => {
                            for event in events {
                                debug!(
                                    "Cough detected: {} at {}ms (conf: {:.2})",
                                    event.label, event.timestamp_ms, event.confidence
                                );
                                session.add_cough();
                                let _ = output_tx.send(BiomarkerOutput::CoughEvent(event));
                            }
                        }
                        Err(e) => {
                            debug!("YAMNet processing error: {}", e);
                        }
                    }
                }

                #[cfg(not(feature = "biomarkers"))]
                let _ = (samples, timestamp_ms); // Silence unused warnings
            }

            BiomarkerInput::Utterance {
                id,
                samples,
                start_ms,
                end_ms,
            } => {
                let mut biomarkers = VocalBiomarkers {
                    utterance_id: id,
                    start_ms,
                    end_ms,
                    ..Default::default()
                };

                // Calculate vitality (pitch variability)
                if config.vitality_enabled {
                    if let Some((vitality, f0_mean, voiced_ratio)) =
                        calculate_vitality(&samples, 16000)
                    {
                        biomarkers.vitality = Some(vitality);
                        biomarkers.f0_mean = Some(f0_mean);
                        biomarkers.voiced_frame_ratio = voiced_ratio;
                        vitality_values.push(vitality);
                        debug!(
                            "Vitality: {:.1} Hz (mean F0: {:.1} Hz, voiced: {:.0}%)",
                            vitality,
                            f0_mean,
                            voiced_ratio * 100.0
                        );
                    }
                }

                // Calculate stability (CPP)
                if config.stability_enabled {
                    if let Some(stability) = calculate_stability(&samples, 16000) {
                        biomarkers.stability = Some(stability);
                        stability_values.push(stability);
                        debug!("Stability (CPP): {:.1} dB", stability);
                    }
                }

                let _ = output_tx.send(BiomarkerOutput::VocalBiomarkers(biomarkers));
            }

            BiomarkerInput::SegmentInfo {
                speaker_id,
                start_ms,
                end_ms,
            } => {
                if config.session_metrics_enabled {
                    session.add_turn(speaker_id.as_deref(), start_ms, end_ms);

                    // Send updated session metrics
                    let mut metrics = session.get_metrics();

                    // Add vitality/stability session means
                    if !vitality_values.is_empty() {
                        metrics.vitality_session_mean = Some(
                            vitality_values.iter().sum::<f32>() / vitality_values.len() as f32,
                        );
                    }
                    if !stability_values.is_empty() {
                        metrics.stability_session_mean = Some(
                            stability_values.iter().sum::<f32>() / stability_values.len() as f32,
                        );
                    }

                    let _ = output_tx.send(BiomarkerOutput::SessionMetrics(metrics));
                }
            }

            BiomarkerInput::Shutdown => {
                info!("Biomarker thread received shutdown signal");
                break;
            }
        }
    }

    // Send final session metrics
    if config.session_metrics_enabled {
        let mut metrics = session.get_metrics();
        if !vitality_values.is_empty() {
            metrics.vitality_session_mean =
                Some(vitality_values.iter().sum::<f32>() / vitality_values.len() as f32);
        }
        if !stability_values.is_empty() {
            metrics.stability_session_mean =
                Some(stability_values.iter().sum::<f32>() / stability_values.len() as f32);
        }
        let _ = output_tx.send(BiomarkerOutput::SessionMetrics(metrics));
    }

    // Cleanup
    #[cfg(feature = "biomarkers")]
    drop(yamnet);

    info!("Biomarker thread stopped");
}
