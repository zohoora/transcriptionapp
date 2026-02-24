//! Session lifecycle commands (start, stop, reset)

use super::listening::SharedListeningState;
use super::{emit_status_arc, emit_transcript_arc, SharedPipelineState, SharedSessionManager};
use crate::activity_log;
use crate::config::Config;
use crate::debug_storage::DebugStorage;
use crate::local_archive;
use crate::pipeline::{start_pipeline, PipelineConfig, PipelineMessage};
use crate::session::{SessionError, SessionState};
use chrono::Utc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Start a transcription session
#[tauri::command]
pub async fn start_session(
    app: AppHandle,
    session_state: State<'_, SharedSessionManager>,
    pipeline_state: State<'_, SharedPipelineState>,
    listening_state: State<'_, SharedListeningState>,
    device_id: Option<String>,
) -> Result<(), String> {
    info!("Starting session with device: {:?}", device_id);

    // Clone the Arcs for use in async context
    let session_arc = session_state.inner().clone();
    let pipeline_arc = pipeline_state.inner().clone();

    // Check for and consume any initial audio buffer from listening mode
    // This is used for optimistic recording - the buffer contains audio captured
    // before the greeting check completed
    let initial_audio_buffer = {
        let mut listening = listening_state.lock().map_err(|e| e.to_string())?;
        listening.initial_audio_buffer.take()
    };

    if let Some(ref buffer) = initial_audio_buffer {
        info!(
            "Consuming initial audio buffer: {} samples ({:.1}s)",
            buffer.len(),
            buffer.len() as f32 / 16000.0
        );
    }

    // Transition to preparing
    {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_preparing().map_err(|e| e.to_string())?;
    }

    // Emit initial status
    emit_status_arc(&app, &session_arc)?;

    // Load config - always uses remote Whisper server, no local model needed
    let config = Config::load_or_default();
    let model_path = config.get_model_path().unwrap_or_default(); // Fallback path for local model (unused with remote transcription)

    // Create pipeline config
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

    // Generate audio file path for recording
    let audio_output_path = {
        let recordings_dir = config.get_recordings_dir();
        if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
            info!(
                "Could not create recordings directory: {}, audio won't be saved",
                e
            );
            None
        } else {
            let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
            let audio_path = recordings_dir.join(format!("session_{}.wav", timestamp));
            Some(audio_path)
        }
    };

    // Store audio path in session
    if let Some(ref path) = audio_output_path {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.set_audio_file_path(path.clone());
    }

    // Prepare device ID for logging before moving into pipeline_config
    let device_id_for_config = if device_id.as_deref() == Some("default") {
        None
    } else {
        device_id
    };
    let device_name_for_log = device_id_for_config.clone();

    let pipeline_config = PipelineConfig {
        device_id: device_id_for_config,
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
        auto_end_enabled: config.auto_end_enabled,
        auto_end_silence_ms: config.auto_end_silence_ms,
        native_stt_shadow_enabled: config.native_stt_shadow_enabled,
    };

    // Create message channel
    let (tx, mut rx) = mpsc::channel::<PipelineMessage>(32);

    // Start the pipeline
    let handle = match start_pipeline(pipeline_config, tx) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to start pipeline: {}", e);
            let mut session = session_arc.lock().map_err(|e| e.to_string())?;
            session.set_error(SessionError::AudioDeviceError(e.to_string()));
            drop(session);
            emit_status_arc(&app, &session_arc)?;
            return Err(e.to_string());
        }
    };

    // Store the pipeline handle and get generation for this pipeline instance
    let expected_generation = {
        let mut ps = pipeline_arc.lock().map_err(|e| e.to_string())?;
        ps.handle = Some(handle);
        ps.next_generation()
    };

    // Transition to recording and get session ID for logging
    let session_id = {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_recording("whisper");
        // Use the session's ID (generated in start_preparing) for log correlation
        session.session_id().unwrap_or("unknown").to_string()
    };

    // Log session start (no PHI - just IDs and metadata)
    activity_log::log_session_start(
        &session_id,
        device_name_for_log.as_deref(),
        &config.whisper_model,
    );

    emit_status_arc(&app, &session_arc)?;

    // Spawn task to handle pipeline messages
    let app_clone = app.clone();
    let session_clone = session_arc.clone();
    let pipeline_clone = pipeline_arc.clone();

    // Load config for archive/debug in the spawned task scope
    let debug_enabled_for_task = config.debug_storage_enabled;
    let session_id_for_task = session_id.clone();

    tokio::spawn(async move {
        let mut auto_end_triggered = false;
        let mut native_stt_shadow_transcript: Option<String> = None;

        while let Some(msg) = rx.recv().await {
            // Check if this pipeline instance is still current
            // If generation has changed (due to reset), discard messages
            let current_generation = match pipeline_clone.lock() {
                Ok(ps) => ps.generation,
                Err(_) => break, // Poisoned lock, exit
            };
            if current_generation != expected_generation {
                info!(
                    "Discarding stale pipeline message (generation {} != {})",
                    expected_generation, current_generation
                );
                continue;
            }

            match msg {
                PipelineMessage::Segment(segment) => {
                    // Log segment metadata only - no transcript text (PHI)
                    info!(
                        "Received segment: {} words ({}ms - {}ms)",
                        segment.text.split_whitespace().count(),
                        segment.start_ms,
                        segment.end_ms
                    );
                    let mut session = match session_clone.lock() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    session.add_segment(segment);
                    drop(session);

                    // Emit transcript update
                    if let Ok(session) = session_clone.lock() {
                        let transcript = session.transcript_update();
                        info!(
                            "Emitting transcript_update: {} chars",
                            transcript.finalized_text.len()
                        );
                        let _ = app_clone.emit("transcript_update", transcript);
                    }
                }
                PipelineMessage::Status {
                    audio_clock_ms: _,
                    pending_count,
                    is_speech_active: _,
                } => {
                    if let Ok(mut session) = session_clone.lock() {
                        session.set_pending_count(pending_count);
                        let status = session.status();
                        let _ = app_clone.emit("session_status", status);
                    }
                }
                PipelineMessage::Biomarker(update) => {
                    // Emit biomarker update to frontend
                    let _ = app_clone.emit("biomarker_update", update);
                }
                PipelineMessage::AudioQuality(snapshot) => {
                    // Emit audio quality update to frontend
                    let _ = app_clone.emit("audio_quality", snapshot);
                }
                PipelineMessage::TranscriptChunk { text } => {
                    // Emit streaming chunk as draft_text for real-time display
                    if let Ok(session) = session_clone.lock() {
                        let mut transcript = session.transcript_update();
                        transcript.draft_text = Some(text);
                        let _ = app_clone.emit("transcript_update", transcript);
                    }
                }
                PipelineMessage::SilenceWarning { silence_ms, remaining_ms } => {
                    // Emit silence warning to frontend for countdown display
                    let _ = app_clone.emit("silence_warning", serde_json::json!({
                        "silence_ms": silence_ms,
                        "remaining_ms": remaining_ms
                    }));
                }
                PipelineMessage::AutoEndSilence { silence_duration_ms } => {
                    // Auto-end triggered due to continuous silence
                    info!(
                        "Auto-end silence detected: {}s of continuous silence",
                        silence_duration_ms / 1000
                    );
                    auto_end_triggered = true;
                    // Emit session_auto_end event to frontend with reason
                    let _ = app_clone.emit("session_auto_end", serde_json::json!({
                        "reason": "silence",
                        "silence_duration_ms": silence_duration_ms
                    }));
                    // Pipeline will self-stop via stop_flag, the Stopped message will follow
                    // and handle session completion
                }
                PipelineMessage::NativeSttShadowTranscript { transcript } => {
                    info!("Received native STT shadow transcript ({} chars)", transcript.len());
                    native_stt_shadow_transcript = Some(transcript);
                }
                PipelineMessage::Stopped => {
                    info!("Pipeline stopped message received, completing session");
                    // Complete the session and emit final status
                    if let Ok(mut session) = session_clone.lock() {
                        // Check if stop_session already completed and archived this session.
                        // This prevents duplicate archival when auto-end races with manual stop.
                        if session.state() == &SessionState::Completed {
                            info!("Session already completed and archived, skipping duplicate archive");
                            // Still save shadow transcript (archive dir already exists from stop_session)
                            if let Some(ref shadow_text) = native_stt_shadow_transcript {
                                if let Ok(archive_dir) = local_archive::get_session_archive_dir(
                                    &session_id_for_task,
                                    &chrono::Utc::now(),
                                ) {
                                    let shadow_path = archive_dir.join("shadow_transcript.txt");
                                    if let Err(e) = std::fs::write(&shadow_path, shadow_text) {
                                        warn!("Failed to save shadow transcript: {}", e);
                                    } else {
                                        info!("Shadow transcript saved to archive ({} chars)", shadow_text.len());
                                    }
                                }
                            }
                            break;
                        }

                        session.complete();
                        let status = session.status();
                        let transcript = session.transcript_update();
                        let _ = app_clone.emit("session_status", status.clone());
                        let _ = app_clone.emit("transcript_update", transcript.clone());

                        // Archive and debug-save (mirrors stop_session logic)
                        // This is critical for auto-ended sessions which bypass stop_session
                        activity_log::log_session_stop(
                            &session_id_for_task,
                            status.elapsed_ms,
                            session.segments().len(),
                            session
                                .audio_file_path()
                                .and_then(|p| std::fs::metadata(p).ok())
                                .map(|m| m.len()),
                        );

                        if debug_enabled_for_task {
                            if let Err(e) = save_session_to_debug_storage(
                                &session_id_for_task,
                                &session,
                                &transcript.finalized_text,
                                status.elapsed_ms,
                            ) {
                                warn!("Failed to save auto-ended session to debug storage: {}", e);
                            }
                        }

                        match local_archive::save_session(
                            &session_id_for_task,
                            &transcript.finalized_text,
                            status.elapsed_ms,
                            session.audio_file_path(),
                            auto_end_triggered,
                            if auto_end_triggered { Some("silence") } else { None },
                        ) {
                            Ok(session_dir) => {
                                // Save native STT shadow transcript into same dir
                                // (uses returned path to avoid midnight UTC date boundary race)
                                if let Some(ref shadow_text) = native_stt_shadow_transcript {
                                    let shadow_path = session_dir.join("shadow_transcript.txt");
                                    if let Err(e) = std::fs::write(&shadow_path, shadow_text) {
                                        warn!("Failed to save shadow transcript: {}", e);
                                    } else {
                                        info!("Shadow transcript saved to archive ({} chars)", shadow_text.len());
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to archive auto-ended session: {}", e);
                            }
                        }
                    }
                    break;
                }
                PipelineMessage::Error(e) => {
                    error!("Pipeline error: {}", e);
                    if let Ok(mut session) = session_clone.lock() {
                        session.set_error(SessionError::TranscriptionError(e));
                        let status = session.status();
                        let _ = app_clone.emit("session_status", status);
                    }
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Stop the current transcription session
#[tauri::command]
pub async fn stop_session(
    app: AppHandle,
    session_state: State<'_, SharedSessionManager>,
    pipeline_state: State<'_, SharedPipelineState>,
) -> Result<(), String> {
    info!("Stopping session");

    // Clone the Arcs for use in async context
    let session_arc = session_state.inner().clone();
    let pipeline_arc = pipeline_state.inner().clone();

    // Get session info for logging before stopping
    let (session_id, elapsed_ms, segment_count) = {
        let session = session_arc.lock().map_err(|e| e.to_string())?;
        let status = session.status();
        (
            // Reuse the session's ID for log correlation (same ID as start)
            session.session_id().unwrap_or("unknown").to_string(),
            status.elapsed_ms,
            session.segments().len(),
        )
    };

    // Transition to stopping
    {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_stopping().map_err(|e| e.to_string())?;
    }

    emit_status_arc(&app, &session_arc)?;

    // Stop the pipeline
    let handle = {
        let mut ps = pipeline_arc.lock().map_err(|e| e.to_string())?;
        ps.handle.take()
    };

    // Get audio file size if available
    let audio_file_size = {
        let session = session_arc.lock().map_err(|e| e.to_string())?;
        session
            .audio_file_path()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
    };

    // Load config to check debug storage setting
    let config = Config::load_or_default();
    let debug_enabled = config.debug_storage_enabled;

    if let Some(h) = handle {
        h.stop();

        // Wait for pipeline to finish in a separate task
        let app_clone = app.clone();
        let session_clone = session_arc.clone();
        let session_id_for_log = session_id.clone();
        let session_id_for_debug = session_id.clone();

        tokio::task::spawn_blocking(move || {
            h.join();

            // Transition to completed
            if let Ok(mut session) = session_clone.lock() {
                session.complete();
                let status = session.status();
                let transcript = session.transcript_update();
                let _ = app_clone.emit("session_status", status.clone());
                let _ = app_clone.emit("transcript_update", transcript.clone());

                // Log session stop (no PHI - just metrics)
                activity_log::log_session_stop(
                    &session_id_for_log,
                    status.elapsed_ms,
                    session.segments().len(),
                    session
                        .audio_file_path()
                        .and_then(|p| std::fs::metadata(p).ok())
                        .map(|m| m.len()),
                );

                // Save to debug storage if enabled
                if debug_enabled {
                    if let Err(e) = save_session_to_debug_storage(
                        &session_id_for_debug,
                        &session,
                        &transcript.finalized_text,
                        status.elapsed_ms,
                    ) {
                        warn!("Failed to save session to debug storage: {}", e);
                    }
                }

                // Save to local archive (always, for calendar history)
                if let Err(e) = local_archive::save_session(
                    &session_id_for_log,
                    &transcript.finalized_text,
                    status.elapsed_ms,
                    session.audio_file_path(),
                    false, // auto_ended - tracked separately via session_auto_end event
                    None,  // auto_end_reason
                ) {
                    warn!("Failed to save session to local archive: {}", e);
                }
            }
        });
    } else {
        // No pipeline running, just complete
        let transcript_text = {
            let mut session = session_arc.lock().map_err(|e| e.to_string())?;
            session.complete();
            session.transcript_update().finalized_text
        };

        // Log session stop
        activity_log::log_session_stop(&session_id, elapsed_ms, segment_count, audio_file_size);

        // Save to debug storage if enabled
        if debug_enabled {
            let session = session_arc.lock().map_err(|e| e.to_string())?;
            if let Err(e) = save_session_to_debug_storage(
                &session_id,
                &session,
                &transcript_text,
                elapsed_ms,
            ) {
                warn!("Failed to save session to debug storage: {}", e);
            }
        }

        // Save to local archive (always, for calendar history)
        {
            let session = session_arc.lock().map_err(|e| e.to_string())?;
            if let Err(e) = local_archive::save_session(
                &session_id,
                &transcript_text,
                elapsed_ms,
                session.audio_file_path(),
                false, // auto_ended
                None,  // auto_end_reason
            ) {
                warn!("Failed to save session to local archive: {}", e);
            }
        }

        emit_status_arc(&app, &session_arc)?;
        emit_transcript_arc(&app, &session_arc)?;
    }

    Ok(())
}

/// Save session data to debug storage
/// This stores transcript, segments, and metadata locally for debugging purposes.
/// IMPORTANT: This stores PHI and should only be used during development.
fn save_session_to_debug_storage(
    session_id: &str,
    session: &crate::session::SessionManager,
    transcript_text: &str,
    elapsed_ms: u64,
) -> Result<(), String> {
    // Create debug storage instance
    let mut debug_storage = DebugStorage::new(session_id, true)?;

    // Add all segments from the session
    for (index, segment) in session.segments().iter().enumerate() {
        debug_storage.add_segment(
            index,
            segment.start_ms,
            segment.end_ms,
            &segment.text,
            segment.speaker_id.clone(),
        );
    }

    // Save the transcript
    debug_storage.save_transcript()?;

    // Copy audio file if it exists
    if let Some(audio_path) = session.audio_file_path() {
        if audio_path.exists() {
            let debug_audio_path = debug_storage.audio_path();
            if let Err(e) = std::fs::copy(audio_path, &debug_audio_path) {
                warn!("Failed to copy audio to debug storage: {}", e);
            } else {
                info!(
                    session_id = %session_id,
                    path = %debug_audio_path.display(),
                    "Audio copied to debug storage"
                );
            }
        }
    }

    // Finalize with session duration
    debug_storage.finalize(elapsed_ms)?;

    info!(
        session_id = %session_id,
        segments = session.segments().len(),
        chars = transcript_text.len(),
        "Session saved to debug storage"
    );

    Ok(())
}

/// Reset the session to idle
#[tauri::command]
pub fn reset_session(
    app: AppHandle,
    session_state: State<'_, SharedSessionManager>,
    pipeline_state: State<'_, SharedPipelineState>,
) -> Result<(), String> {
    info!("Resetting session");

    // Generate session ID for logging
    let session_id = Some(uuid::Uuid::new_v4().to_string());

    // Stop any running pipeline and increment generation
    // The generation increment ensures any in-flight pipeline messages are discarded
    {
        let mut ps = pipeline_state.lock().map_err(|e| e.to_string())?;
        // Increment generation first so receiver task will discard any pending messages
        ps.next_generation();
        if let Some(h) = ps.handle.take() {
            h.stop();
            // Join in a background thread to avoid blocking the Tauri command thread.
            // Drop would also join, but that blocks the current thread.
            std::thread::spawn(move || {
                h.join();
            });
        }
    }

    // Reset session
    {
        let mut session = session_state.lock().map_err(|e| e.to_string())?;
        session.reset();
    }

    // Log session reset
    activity_log::log_session_reset(session_id.as_deref());

    emit_status_arc(&app, session_state.inner())?;
    emit_transcript_arc(&app, session_state.inner())?;

    Ok(())
}

/// Get the audio file path for the current session
#[tauri::command]
pub fn get_audio_file_path(
    session_state: State<'_, SharedSessionManager>,
) -> Result<Option<String>, String> {
    let session = session_state.lock().map_err(|e| e.to_string())?;
    Ok(session
        .audio_file_path()
        .map(|p| p.to_string_lossy().to_string()))
}

/// Reset the silence timer to cancel auto-end countdown
/// Called when user clicks "Keep Recording" button during silence warning
#[tauri::command]
pub fn reset_silence_timer(
    pipeline_state: State<'_, SharedPipelineState>,
) -> Result<(), String> {
    info!("Resetting silence timer (user cancelled auto-end)");

    let ps = pipeline_state.lock().map_err(|e| e.to_string())?;
    if let Some(ref handle) = ps.handle {
        handle.reset_silence_timer();
        Ok(())
    } else {
        Err("No active recording session".to_string())
    }
}
