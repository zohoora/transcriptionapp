//! Tauri commands for manual audio file upload and batch processing.
//!
//! Processes uploaded audio files through the same pipeline as mobile recordings:
//! ffmpeg transcode → STT batch → encounter detection → SOAP generation → archive.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tracing::{info, warn};

use super::{CommandError, SharedActivePhysician, SharedProfileClient};
use crate::audio_processing;
use crate::commands::physicians::SharedServerConfig;
use crate::config::Config;
use crate::llm_client::{LLMClient, SoapFormat, SoapOptions};
use crate::local_archive::{self, ArchiveMetadata};
use crate::server_config_resolve::resolve;
use crate::whisper_server::WhisperServerClient;

/// Result of processing an uploaded audio file.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AudioUploadResult {
    pub sessions: Vec<UploadedSession>,
    pub total_word_count: usize,
}

/// Info about a single session created from the upload.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UploadedSession {
    pub session_id: String,
    pub encounter_number: u32,
    pub word_count: usize,
    pub has_soap: bool,
}

/// Progress event emitted during upload processing.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AudioUploadProgress {
    step: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    encounter: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl AudioUploadProgress {
    fn step(step: &str) -> Self {
        Self {
            step: step.to_string(),
            encounter: None,
            total: None,
            error: None,
        }
    }

    fn soap(encounter: u32, total: u32) -> Self {
        Self {
            step: "generating_soap".to_string(),
            encounter: Some(encounter),
            total: Some(total),
            error: None,
        }
    }
}

fn emit_progress(app: &AppHandle, progress: &AudioUploadProgress) {
    let _ = app.emit("audio_upload_progress", progress);
}

/// Check if ffmpeg is available on this system.
#[tauri::command]
pub async fn check_audio_ffmpeg() -> Result<bool, CommandError> {
    Ok(audio_processing::check_ffmpeg_available().is_ok())
}

/// Process an uploaded audio file through the batch pipeline.
///
/// Steps: validate → transcode → transcribe → detect encounters → generate SOAP → archive.
#[tauri::command]
pub async fn process_audio_upload(
    app: AppHandle,
    file_path: String,
    recording_date: String,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
    server_config: State<'_, SharedServerConfig>,
) -> Result<AudioUploadResult, CommandError> {
    // ── Validate inputs ──────────────────────────────────────────────────
    let input_path = PathBuf::from(&file_path)
        .canonicalize()
        .map_err(|_| CommandError::Validation("File not found or inaccessible".into()))?;
    if !audio_processing::is_supported_format(&input_path) {
        return Err(CommandError::Validation(format!(
            "Unsupported audio format. Supported: {}",
            audio_processing::SUPPORTED_EXTENSIONS.join(", ")
        )));
    }

    let date = super::parse_date(&recording_date)?;
    let date_str = format!(
        "{:04}-{:02}-{:02}",
        date.format("%Y"),
        date.format("%m"),
        date.format("%d")
    );

    // Check ffmpeg
    audio_processing::check_ffmpeg_available()
        .map_err(|e| CommandError::Other(e))?;

    // Read config for STT/LLM URLs
    let config = Config::load_or_default();

    // Read physician info
    let physician = active_physician.read().await;
    let physician_id = physician.as_ref().map(|p| p.id.clone());
    let physician_name = physician.as_ref().map(|p| p.name.clone());
    let soap_detail = physician.as_ref()
        .and_then(|p| p.soap_detail_level)
        .unwrap_or(5);
    let soap_format = physician.as_ref()
        .and_then(|p| p.soap_format.clone())
        .map(|f| SoapFormat::from_config_str(&f))
        .unwrap_or_default();
    let soap_custom = physician.as_ref()
        .and_then(|p| p.soap_custom_instructions.clone())
        .unwrap_or_default();
    drop(physician);

    info!(
        file = %file_path,
        date = %recording_date,
        physician = ?physician_name,
        "Starting audio upload processing"
    );

    // ── Transcode ────────────────────────────────────────────────────────
    emit_progress(&app, &AudioUploadProgress::step("transcoding"));

    let temp_dir = std::env::temp_dir();
    let temp_id = uuid::Uuid::new_v4().to_string();
    let wav_path = temp_dir.join(format!("upload_{temp_id}.wav"));

    audio_processing::transcode_to_wav(&input_path, &wav_path)
        .map_err(|e| {
            let _ = fs::remove_file(&wav_path); // clean up partial output
            emit_progress(&app, &AudioUploadProgress {
                step: "failed".to_string(),
                error: Some(e.clone()),
                encounter: None,
                total: None,
            });
            CommandError::Other(e)
        })?;

    // ── Transcribe ───────────────────────────────────────────────────────
    emit_progress(&app, &AudioUploadProgress::step("transcribing"));

    let samples = audio_processing::read_wav_samples(&wav_path)
        .map_err(|e| {
            let _ = fs::remove_file(&wav_path);
            CommandError::Other(e)
        })?;

    let stt_client = WhisperServerClient::new(&config.whisper_server_url, &config.stt_alias)
        .map_err(|e| {
            let _ = fs::remove_file(&wav_path);
            CommandError::Network(e)
        })?;

    let transcript = stt_client
        .transcribe_batch(&samples, &config.stt_alias, config.stt_postprocess)
        .await
        .map_err(|e| {
            let _ = fs::remove_file(&wav_path);
            emit_progress(&app, &AudioUploadProgress {
                step: "failed".to_string(),
                error: Some(e.clone()),
                encounter: None,
                total: None,
            });
            CommandError::Network(e)
        })?;

    // Clean up temp WAV — no longer needed
    let _ = fs::remove_file(&wav_path);

    if transcript.trim().is_empty() {
        emit_progress(&app, &AudioUploadProgress {
            step: "failed".to_string(),
            error: Some("No speech detected in audio file".to_string()),
            encounter: None,
            total: None,
        });
        return Err(CommandError::Validation(
            "No speech detected in audio file".into(),
        ));
    }

    let total_word_count = transcript.split_whitespace().count();
    info!("Transcription complete: {total_word_count} words");

    // ── Detect encounters ────────────────────────────────────────────────
    emit_progress(&app, &AudioUploadProgress::step("detecting"));
    let encounters = audio_processing::split_transcript_into_encounters(&transcript);
    let encounter_count = encounters.len() as u32;
    info!("Detected {encounter_count} encounter(s)");

    // ── Generate SOAP + archive per encounter ────────────────────────────
    // Phase 3: model aliases resolved via precedence rule. Snapshot once per
    // upload call — changes take effect on next upload.
    let sc = server_config.read().await;
    let effective_fast_model = resolve(
        Some(&sc.defaults.fast_model),
        &config.fast_model,
        "fast_model",
        &config.user_edited_fields,
    );
    let effective_soap_model = resolve(
        Some(&sc.defaults.soap_model),
        &config.soap_model,
        "soap_model",
        &config.user_edited_fields,
    );
    drop(sc);
    let llm_client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &effective_fast_model,
    )
    .map_err(|e| CommandError::Network(e))?;

    let soap_options = SoapOptions {
        detail_level: soap_detail,
        format: soap_format,
        custom_instructions: soap_custom,
        ..Default::default()
    };

    let mut created_sessions = Vec::new();

    for (idx, encounter_transcript) in encounters.iter().enumerate() {
        let encounter_number = (idx as u32) + 1;
        emit_progress(&app, &AudioUploadProgress::soap(encounter_number, encounter_count));

        let session_id = uuid::Uuid::new_v4().to_string();
        let word_count = encounter_transcript.split_whitespace().count();

        // Create session directory under user-specified date
        let session_dir = local_archive::get_session_archive_dir(&session_id, &date)
            .map_err(|e| CommandError::Io(e))?;
        fs::create_dir_all(&session_dir)
            .map_err(|e| CommandError::Io(e.to_string()))?;

        // Write transcript
        let transcript_path = session_dir.join("transcript.txt");
        let mut file = File::create(&transcript_path)
            .map_err(|e| CommandError::Io(e.to_string()))?;
        file.write_all(encounter_transcript.as_bytes())
            .map_err(|e| CommandError::Io(e.to_string()))?;

        // Build metadata
        let mut metadata = ArchiveMetadata::new(&session_id);
        metadata.started_at = date.to_rfc3339();
        metadata.word_count = word_count;
        metadata.charting_mode = Some("upload".to_string());
        metadata.encounter_number = Some(encounter_number);
        metadata.detection_method = Some("batch".to_string());
        metadata.physician_id = physician_id.clone();
        metadata.physician_name = physician_name.clone();

        // Generate SOAP (fail-open: skip on error)
        let mut has_soap = false;
        if word_count >= 50 {
            match llm_client
                .generate_soap_note(
                    &effective_soap_model,
                    encounter_transcript,
                    None,
                    Some(&soap_options),
                    None,
                )
                .await
            {
                Ok(soap_note) => {
                    let soap_path = session_dir.join("soap_note.txt");
                    if let Err(e) = fs::write(&soap_path, &soap_note.content) {
                        warn!("Failed to write SOAP for encounter {encounter_number}: {e}");
                    } else {
                        metadata.has_soap_note = true;
                        has_soap = true;
                        info!(
                            "SOAP generated for encounter {encounter_number} ({} chars)",
                            soap_note.content.len()
                        );
                    }
                }
                Err(e) => {
                    warn!("SOAP generation failed for encounter {encounter_number}: {e}");
                }
            }
        }

        // Save metadata
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        let metadata_path = session_dir.join("metadata.json");
        fs::write(&metadata_path, &metadata_json)
            .map_err(|e| CommandError::Io(e.to_string()))?;

        info!(
            session_id = %&session_id[..8],
            encounter = encounter_number,
            words = word_count,
            soap = has_soap,
            "Upload session archived"
        );

        // Sync to profile service (fire-and-forget)
        if let Some(ref phys_id) = physician_id {
            let client_guard = profile_client.read().await;
            if let Some(ref client) = *client_guard {
                let client = client.clone();
                let phys_id = phys_id.clone();
                let sid = session_id.clone();
                let ds = date_str.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(details) = local_archive::get_session(&sid, &ds) {
                        if let Ok(body) = serde_json::to_value(&details) {
                            if let Err(e) = client.upload_session(&phys_id, &sid, &body).await {
                                warn!("Server sync failed for upload session {}: {e}", &sid[..8]);
                            }
                        }
                    }
                });
            }
        }

        created_sessions.push(UploadedSession {
            session_id,
            encounter_number,
            word_count,
            has_soap,
        });
    }

    // ── Complete ─────────────────────────────────────────────────────────
    emit_progress(&app, &AudioUploadProgress::step("complete"));

    info!(
        encounters = created_sessions.len(),
        total_words = total_word_count,
        "Audio upload processing complete"
    );

    Ok(AudioUploadResult {
        sessions: created_sessions,
        total_word_count,
    })
}
