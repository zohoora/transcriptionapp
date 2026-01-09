//! Medplum EMR integration commands

use super::SharedMedplumClient;
use crate::activity_log;
use crate::config::Config;
use crate::medplum::{
    AuthState, AuthUrl, Encounter, EncounterDetails, EncounterSummary, MedplumClient, Patient,
    SyncResult, SyncStatus,
};
use crate::ollama::MultiPatientSoapResult;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;
use tracing::info;

/// Get or create the Medplum client, initializing if needed
async fn get_or_create_medplum_client(medplum_state: &SharedMedplumClient) -> Result<(), String> {
    let mut client_guard = medplum_state.write().await;
    if client_guard.is_none() {
        let config = Config::load_or_default();
        if config.medplum_client_id.is_empty() {
            return Err(
                "Medplum client ID not configured. Please set it in settings.".to_string(),
            );
        }
        let client = MedplumClient::new(&config.medplum_server_url, &config.medplum_client_id)
            .map_err(|e| e.to_string())?;
        *client_guard = Some(client);
    }
    Ok(())
}

/// Get the current Medplum authentication state
#[tauri::command]
pub async fn medplum_get_auth_state(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<AuthState, String> {
    let client_guard = medplum_state.read().await;
    if let Some(ref client) = *client_guard {
        Ok(client.get_auth_state().await)
    } else {
        Ok(AuthState::default())
    }
}

/// Try to restore a previous Medplum session (auto-refresh if needed)
/// Call this on app startup to check if user is already logged in
#[tauri::command]
pub async fn medplum_try_restore_session(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<AuthState, String> {
    info!("Attempting to restore Medplum session...");

    get_or_create_medplum_client(medplum_state.inner()).await?;

    let client_guard = medplum_state.read().await;
    if let Some(ref client) = *client_guard {
        let auth_state = client.try_restore_session().await;
        activity_log::log_medplum_auth(
            "restore",
            auth_state.practitioner_id.as_deref(),
            auth_state.is_authenticated,
            None,
        );
        Ok(auth_state)
    } else {
        activity_log::log_medplum_auth("restore", None, false, Some("Client not initialized"));
        Ok(AuthState::default())
    }
}

/// Start the Medplum OAuth authorization flow
/// Returns the authorization URL to open in a browser
#[tauri::command]
pub async fn medplum_start_auth(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<AuthUrl, String> {
    info!("Starting Medplum OAuth flow");

    activity_log::log_medplum_auth("login_start", None, true, None);

    get_or_create_medplum_client(medplum_state.inner()).await?;

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client.start_auth_flow().await.map_err(|e| {
        activity_log::log_medplum_auth("login_start", None, false, Some(&e.to_string()));
        e.to_string()
    })
}

/// Maximum length for OAuth parameters (prevent DoS)
const MAX_OAUTH_PARAM_LENGTH: usize = 2048;

/// Handle OAuth callback with authorization code
#[tauri::command]
pub async fn medplum_handle_callback(
    medplum_state: State<'_, SharedMedplumClient>,
    code: String,
    state: String,
) -> Result<AuthState, String> {
    info!("Handling Medplum OAuth callback");

    // Validate OAuth parameters
    if code.is_empty() {
        return Err("OAuth authorization code is required".to_string());
    }
    if state.is_empty() {
        return Err("OAuth state parameter is required".to_string());
    }
    if code.len() > MAX_OAUTH_PARAM_LENGTH {
        return Err("OAuth authorization code exceeds maximum length".to_string());
    }
    if state.len() > MAX_OAUTH_PARAM_LENGTH {
        return Err("OAuth state parameter exceeds maximum length".to_string());
    }

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    match client.exchange_code(&code, &state).await {
        Ok(auth_state) => {
            activity_log::log_medplum_auth(
                "login_complete",
                auth_state.practitioner_id.as_deref(),
                true,
                None,
            );
            Ok(auth_state)
        }
        Err(e) => {
            activity_log::log_medplum_auth("login_complete", None, false, Some(&e.to_string()));
            Err(e.to_string())
        }
    }
}

/// Logout from Medplum
#[tauri::command]
pub async fn medplum_logout(medplum_state: State<'_, SharedMedplumClient>) -> Result<(), String> {
    info!("Logging out from Medplum");

    let client_guard = medplum_state.read().await;
    if let Some(ref client) = *client_guard {
        client.logout().await;
        activity_log::log_medplum_auth("logout", None, true, None);
    }
    Ok(())
}

/// Refresh the Medplum access token
#[tauri::command]
pub async fn medplum_refresh_token(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<AuthState, String> {
    info!("Refreshing Medplum access token");

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    match client.refresh_token().await {
        Ok(auth_state) => {
            activity_log::log_medplum_auth(
                "refresh",
                auth_state.practitioner_id.as_deref(),
                true,
                None,
            );
            Ok(auth_state)
        }
        Err(e) => {
            activity_log::log_medplum_auth("refresh", None, false, Some(&e.to_string()));
            Err(e.to_string())
        }
    }
}

/// Search for patients by name or MRN
#[tauri::command]
pub async fn medplum_search_patients(
    medplum_state: State<'_, SharedMedplumClient>,
    query: String,
) -> Result<Vec<Patient>, String> {
    info!("Searching for patients: {}", query);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client
        .search_patients(&query)
        .await
        .map_err(|e| e.to_string())
}

/// Create a new encounter for a patient
#[tauri::command]
pub async fn medplum_create_encounter(
    medplum_state: State<'_, SharedMedplumClient>,
    patient_id: String,
) -> Result<Encounter, String> {
    info!("Creating encounter for patient: {}", patient_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client
        .create_encounter(&patient_id)
        .await
        .map_err(|e| e.to_string())
}

/// Complete an encounter with transcript, SOAP note, and optional audio
#[tauri::command]
pub async fn medplum_complete_encounter(
    medplum_state: State<'_, SharedMedplumClient>,
    encounter_id: String,
    encounter_fhir_id: String,
    patient_id: String,
    transcript: String,
    soap_note: Option<String>,
    audio_data: Option<Vec<u8>>,
) -> Result<SyncResult, String> {
    info!("Completing encounter: {}", encounter_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    let mut sync_status = SyncStatus::default();
    let mut errors = Vec::new();

    // Upload transcript (log size, not content)
    let transcript_size = transcript.len();
    match client
        .upload_transcript(&encounter_id, &encounter_fhir_id, &patient_id, &transcript)
        .await
    {
        Ok(doc_id) => {
            sync_status.transcript_synced = true;
            activity_log::log_document_upload(
                &encounter_id,
                &encounter_fhir_id,
                "transcript",
                &doc_id,
                transcript_size,
                true,
                None,
            );
        }
        Err(e) => {
            activity_log::log_document_upload(
                &encounter_id,
                &encounter_fhir_id,
                "transcript",
                "",
                transcript_size,
                false,
                Some(&e.to_string()),
            );
            errors.push(format!("Transcript: {}", e));
        }
    }

    // Upload SOAP note if provided (log size, not content)
    if let Some(ref soap) = soap_note {
        let soap_size = soap.len();
        match client
            .upload_soap_note(&encounter_id, &encounter_fhir_id, &patient_id, soap)
            .await
        {
            Ok(doc_id) => {
                sync_status.soap_note_synced = true;
                activity_log::log_document_upload(
                    &encounter_id,
                    &encounter_fhir_id,
                    "soap_note",
                    &doc_id,
                    soap_size,
                    true,
                    None,
                );
            }
            Err(e) => {
                activity_log::log_document_upload(
                    &encounter_id,
                    &encounter_fhir_id,
                    "soap_note",
                    "",
                    soap_size,
                    false,
                    Some(&e.to_string()),
                );
                errors.push(format!("SOAP note: {}", e));
            }
        }
    } else {
        sync_status.soap_note_synced = true; // Not required
    }

    // Upload audio if provided
    if let Some(ref audio) = audio_data {
        let audio_size = audio.len();
        match client
            .upload_audio(
                &encounter_id,
                &encounter_fhir_id,
                &patient_id,
                audio,
                "audio/webm",
                None,
            )
            .await
        {
            Ok(media_id) => {
                sync_status.audio_synced = true;
                activity_log::log_audio_upload(
                    &encounter_id,
                    &encounter_fhir_id,
                    &media_id,
                    "", // binary_id not returned
                    audio_size,
                    None,
                    true,
                    None,
                );
            }
            Err(e) => {
                activity_log::log_audio_upload(
                    &encounter_id,
                    &encounter_fhir_id,
                    "",
                    "",
                    audio_size,
                    None,
                    false,
                    Some(&e.to_string()),
                );
                errors.push(format!("Audio: {}", e));
            }
        }
    } else {
        sync_status.audio_synced = true; // Not required
    }

    // Complete the encounter
    match client.complete_encounter(&encounter_fhir_id).await {
        Ok(_) => {
            sync_status.encounter_synced = true;
            activity_log::log_encounter_sync(
                &encounter_id,
                &encounter_id,
                &encounter_fhir_id,
                "complete",
                true,
                None,
            );
        }
        Err(e) => {
            activity_log::log_encounter_sync(
                &encounter_id,
                &encounter_id,
                &encounter_fhir_id,
                "complete",
                false,
                Some(&e.to_string()),
            );
            errors.push(format!("Encounter: {}", e));
        }
    }

    sync_status.last_sync_time = Some(chrono::Utc::now().to_rfc3339());

    let success = errors.is_empty();
    let error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    Ok(SyncResult {
        success,
        status: sync_status,
        error,
        encounter_id: Some(encounter_id.clone()),
        encounter_fhir_id: Some(encounter_fhir_id.clone()),
    })
}

/// Get encounter history for the current practitioner
#[tauri::command]
pub async fn medplum_get_encounter_history(
    medplum_state: State<'_, SharedMedplumClient>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<Vec<EncounterSummary>, String> {
    info!("Getting encounter history");

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client
        .get_encounter_history(start_date.as_deref(), end_date.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Get detailed encounter data including documents
#[tauri::command]
pub async fn medplum_get_encounter_details(
    medplum_state: State<'_, SharedMedplumClient>,
    encounter_id: String,
) -> Result<EncounterDetails, String> {
    info!("Getting encounter details: {}", encounter_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client
        .get_encounter_details(&encounter_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get raw audio data from Medplum Binary resource
#[tauri::command]
pub async fn medplum_get_audio_data(
    medplum_state: State<'_, SharedMedplumClient>,
    binary_id: String,
) -> Result<Vec<u8>, String> {
    info!("Fetching audio data: {}", binary_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client
        .get_audio_data(&binary_id)
        .await
        .map_err(|e| e.to_string())
}

/// Manual sync of an encounter
#[tauri::command]
pub async fn medplum_sync_encounter(
    medplum_state: State<'_, SharedMedplumClient>,
    encounter_id: String,
    encounter_fhir_id: String,
    patient_id: String,
    transcript: String,
    soap_note: Option<String>,
    audio_data: Option<Vec<u8>>,
) -> Result<SyncResult, String> {
    info!("Manual sync for encounter: {}", encounter_id);

    // Reuse the complete_encounter logic
    medplum_complete_encounter(
        medplum_state,
        encounter_id,
        encounter_fhir_id,
        patient_id,
        transcript,
        soap_note,
        audio_data,
    )
    .await
}

/// Quick sync - creates placeholder patient, encounter, and uploads everything in one call
#[tauri::command]
pub async fn medplum_quick_sync(
    medplum_state: State<'_, SharedMedplumClient>,
    transcript: String,
    soap_note: Option<String>,
    audio_file_path: Option<String>,
    session_duration_ms: u64,
) -> Result<SyncResult, String> {
    info!("Quick sync: creating placeholder patient and encounter");

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    // Step 1: Create placeholder patient
    let patient = client
        .create_placeholder_patient()
        .await
        .map_err(|e| format!("Failed to create placeholder patient: {}", e))?;

    info!("Created placeholder patient: {}", patient.id);

    // Step 2: Create encounter
    let encounter = client
        .create_encounter(&patient.id)
        .await
        .map_err(|e| format!("Failed to create encounter: {}", e))?;

    info!("Created encounter: {}", encounter.id);

    // Log encounter creation
    activity_log::log_encounter_sync(
        &encounter.id,
        &encounter.id,
        &encounter.id,
        "create",
        true,
        None,
    );

    // Step 3: Upload transcript and SOAP note
    let mut sync_status = SyncStatus::default();
    let mut errors = Vec::new();

    // Upload transcript (log size, not content)
    let transcript_size = transcript.len();
    match client
        .upload_transcript(&encounter.id, &encounter.id, &patient.id, &transcript)
        .await
    {
        Ok(doc_id) => {
            sync_status.transcript_synced = true;
            activity_log::log_document_upload(
                &encounter.id,
                &encounter.id,
                "transcript",
                &doc_id,
                transcript_size,
                true,
                None,
            );
            info!("Transcript uploaded successfully");
        }
        Err(e) => {
            activity_log::log_document_upload(
                &encounter.id,
                &encounter.id,
                "transcript",
                "",
                transcript_size,
                false,
                Some(&e.to_string()),
            );
            errors.push(format!("Transcript: {}", e));
        }
    }

    // Upload SOAP note if provided (log size, not content)
    if let Some(ref soap) = soap_note {
        let soap_size = soap.len();
        match client
            .upload_soap_note(&encounter.id, &encounter.id, &patient.id, soap)
            .await
        {
            Ok(doc_id) => {
                sync_status.soap_note_synced = true;
                activity_log::log_document_upload(
                    &encounter.id,
                    &encounter.id,
                    "soap_note",
                    &doc_id,
                    soap_size,
                    true,
                    None,
                );
                info!("SOAP note uploaded successfully");
            }
            Err(e) => {
                activity_log::log_document_upload(
                    &encounter.id,
                    &encounter.id,
                    "soap_note",
                    "",
                    soap_size,
                    false,
                    Some(&e.to_string()),
                );
                errors.push(format!("SOAP note: {}", e));
            }
        }
    } else {
        sync_status.soap_note_synced = true; // Not applicable
    }

    // Upload audio if provided
    if let Some(ref audio_path) = audio_file_path {
        let path = PathBuf::from(audio_path);
        if path.exists() {
            match std::fs::read(&path) {
                Ok(audio_data) => {
                    let audio_size = audio_data.len();
                    let duration_seconds = Some(session_duration_ms / 1000);
                    match client
                        .upload_audio(
                            &encounter.id,
                            &encounter.id,
                            &patient.id,
                            &audio_data,
                            "audio/wav",
                            duration_seconds,
                        )
                        .await
                    {
                        Ok(media_id) => {
                            sync_status.audio_synced = true;
                            activity_log::log_audio_upload(
                                &encounter.id,
                                &encounter.id,
                                &media_id,
                                "",
                                audio_size,
                                duration_seconds,
                                true,
                                None,
                            );
                            info!("Audio uploaded successfully");
                        }
                        Err(e) => {
                            activity_log::log_audio_upload(
                                &encounter.id,
                                &encounter.id,
                                "",
                                "",
                                audio_size,
                                duration_seconds,
                                false,
                                Some(&e.to_string()),
                            );
                            errors.push(format!("Audio: {}", e));
                        }
                    }
                }
                Err(e) => errors.push(format!("Audio read error: {}", e)),
            }
        } else {
            info!(
                "Audio file not found at {:?}, skipping audio upload",
                path
            );
            sync_status.audio_synced = true; // Not available
        }
    } else {
        sync_status.audio_synced = true; // Not applicable
    }

    // Mark encounter as synced
    sync_status.encounter_synced = true;
    sync_status.last_sync_time = Some(chrono::Utc::now().to_rfc3339());

    let success = errors.is_empty();
    let error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    info!(
        "Quick sync complete. Success: {}, Transcript: {}, SOAP: {}",
        success, sync_status.transcript_synced, sync_status.soap_note_synced
    );

    Ok(SyncResult {
        success,
        status: sync_status,
        error,
        encounter_id: Some(encounter.id.clone()),
        encounter_fhir_id: Some(encounter.id.clone()),
    })
}

/// Add a SOAP note to an existing encounter
/// Used when SOAP is generated after initial sync
#[tauri::command]
pub async fn medplum_add_soap_to_encounter(
    medplum_state: State<'_, SharedMedplumClient>,
    encounter_fhir_id: String,
    soap_note: String,
) -> Result<bool, String> {
    info!("Adding SOAP note to existing encounter: {}", encounter_fhir_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    // We need to get the patient ID from the encounter
    // For now, we'll use a placeholder since the encounter already has the context
    let soap_size = soap_note.len();

    match client
        .upload_soap_note(&encounter_fhir_id, &encounter_fhir_id, &encounter_fhir_id, &soap_note)
        .await
    {
        Ok(doc_id) => {
            activity_log::log_document_upload(
                &encounter_fhir_id,
                &encounter_fhir_id,
                "soap_note",
                &doc_id,
                soap_size,
                true,
                None,
            );
            info!("SOAP note added to encounter successfully");
            Ok(true)
        }
        Err(e) => {
            activity_log::log_document_upload(
                &encounter_fhir_id,
                &encounter_fhir_id,
                "soap_note",
                "",
                soap_size,
                false,
                Some(&e.to_string()),
            );
            Err(format!("Failed to add SOAP note: {}", e))
        }
    }
}

/// Info about a synced patient in multi-patient sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientSyncInfo {
    /// Label from SOAP result (e.g., "Patient 1")
    pub patient_label: String,
    /// Speaker ID from transcript (e.g., "Speaker 1")
    pub speaker_id: String,
    /// Created patient's FHIR ID
    pub patient_fhir_id: String,
    /// Created encounter's FHIR ID
    pub encounter_fhir_id: String,
    /// Whether SOAP note was synced
    pub has_soap: bool,
}

/// Result of multi-patient sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPatientSyncResult {
    /// Whether all syncs succeeded
    pub success: bool,
    /// Info about each synced patient/encounter
    pub patients: Vec<PatientSyncInfo>,
    /// Error message if any patient sync failed
    pub error: Option<String>,
}

/// Multi-patient quick sync - creates placeholder patient and encounter for each patient
/// in the multi-patient SOAP result, then uploads transcript (to first encounter),
/// patient-specific SOAP notes, and audio (to first encounter)
#[tauri::command]
pub async fn medplum_multi_patient_quick_sync(
    medplum_state: State<'_, SharedMedplumClient>,
    transcript: String,
    soap_result: MultiPatientSoapResult,
    audio_file_path: Option<String>,
    session_duration_ms: u64,
) -> Result<MultiPatientSyncResult, String> {
    info!(
        "Multi-patient quick sync: {} patients, physician: {:?}",
        soap_result.notes.len(),
        soap_result.physician_speaker
    );

    if soap_result.notes.is_empty() {
        return Err("No patients to sync".to_string());
    }

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    let mut synced_patients: Vec<PatientSyncInfo> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Create a patient and encounter for each patient in the SOAP result
    for (i, patient_note) in soap_result.notes.iter().enumerate() {
        info!(
            "Creating patient/encounter for: {} ({})",
            patient_note.patient_label, patient_note.speaker_id
        );

        // Step 1: Create placeholder patient with label
        let patient = match client.create_placeholder_patient().await {
            Ok(p) => p,
            Err(e) => {
                errors.push(format!(
                    "Failed to create patient for {}: {}",
                    patient_note.patient_label, e
                ));
                continue;
            }
        };

        info!("Created placeholder patient: {}", patient.id);

        // Step 2: Create encounter
        let encounter = match client.create_encounter(&patient.id).await {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!(
                    "Failed to create encounter for {}: {}",
                    patient_note.patient_label, e
                ));
                continue;
            }
        };

        info!("Created encounter: {}", encounter.id);

        // Log encounter creation
        activity_log::log_encounter_sync(
            &encounter.id,
            &encounter.id,
            &encounter.id,
            "create",
            true,
            None,
        );

        let mut has_soap = false;

        // Step 3: Upload transcript only to first encounter (shared transcript)
        if i == 0 {
            let transcript_size = transcript.len();
            match client
                .upload_transcript(&encounter.id, &encounter.id, &patient.id, &transcript)
                .await
            {
                Ok(doc_id) => {
                    activity_log::log_document_upload(
                        &encounter.id,
                        &encounter.id,
                        "transcript",
                        &doc_id,
                        transcript_size,
                        true,
                        None,
                    );
                    info!("Transcript uploaded to first encounter");
                }
                Err(e) => {
                    activity_log::log_document_upload(
                        &encounter.id,
                        &encounter.id,
                        "transcript",
                        "",
                        transcript_size,
                        false,
                        Some(&e.to_string()),
                    );
                    errors.push(format!("Transcript upload failed: {}", e));
                }
            }
        }

        // Step 4: Upload patient-specific SOAP note
        // Format the SOAP note as a string
        let soap_text = format!(
            "SUBJECTIVE:\n{}\n\nOBJECTIVE:\n{}\n\nASSESSMENT:\n{}\n\nPLAN:\n{}",
            patient_note.soap.subjective,
            patient_note.soap.objective,
            patient_note.soap.assessment,
            patient_note.soap.plan
        );
        let soap_size = soap_text.len();

        match client
            .upload_soap_note(&encounter.id, &encounter.id, &patient.id, &soap_text)
            .await
        {
            Ok(doc_id) => {
                has_soap = true;
                activity_log::log_document_upload(
                    &encounter.id,
                    &encounter.id,
                    "soap_note",
                    &doc_id,
                    soap_size,
                    true,
                    None,
                );
                info!("SOAP note uploaded for {}", patient_note.patient_label);
            }
            Err(e) => {
                activity_log::log_document_upload(
                    &encounter.id,
                    &encounter.id,
                    "soap_note",
                    "",
                    soap_size,
                    false,
                    Some(&e.to_string()),
                );
                errors.push(format!(
                    "SOAP upload for {} failed: {}",
                    patient_note.patient_label, e
                ));
            }
        }

        // Step 5: Upload audio only to first encounter
        if i == 0 {
            if let Some(ref audio_path) = audio_file_path {
                let path = PathBuf::from(audio_path);
                if path.exists() {
                    match std::fs::read(&path) {
                        Ok(audio_data) => {
                            let audio_size = audio_data.len();
                            let duration_seconds = Some(session_duration_ms / 1000);
                            match client
                                .upload_audio(
                                    &encounter.id,
                                    &encounter.id,
                                    &patient.id,
                                    &audio_data,
                                    "audio/wav",
                                    duration_seconds,
                                )
                                .await
                            {
                                Ok(media_id) => {
                                    activity_log::log_audio_upload(
                                        &encounter.id,
                                        &encounter.id,
                                        &media_id,
                                        "",
                                        audio_size,
                                        duration_seconds,
                                        true,
                                        None,
                                    );
                                    info!("Audio uploaded to first encounter");
                                }
                                Err(e) => {
                                    activity_log::log_audio_upload(
                                        &encounter.id,
                                        &encounter.id,
                                        "",
                                        "",
                                        audio_size,
                                        duration_seconds,
                                        false,
                                        Some(&e.to_string()),
                                    );
                                    errors.push(format!("Audio upload failed: {}", e));
                                }
                            }
                        }
                        Err(e) => errors.push(format!("Audio read error: {}", e)),
                    }
                } else {
                    info!("Audio file not found at {:?}, skipping", path);
                }
            }
        }

        // Record synced patient info
        synced_patients.push(PatientSyncInfo {
            patient_label: patient_note.patient_label.clone(),
            speaker_id: patient_note.speaker_id.clone(),
            patient_fhir_id: patient.id.clone(),
            encounter_fhir_id: encounter.id.clone(),
            has_soap,
        });
    }

    let success = errors.is_empty();
    let error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    info!(
        "Multi-patient sync complete. Success: {}, Patients synced: {}",
        success,
        synced_patients.len()
    );

    Ok(MultiPatientSyncResult {
        success,
        patients: synced_patients,
        error,
    })
}

/// Check if Medplum server is reachable (doesn't require authentication)
#[tauri::command]
pub async fn medplum_check_connection(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<bool, String> {
    let config = Config::load_or_default();
    if config.medplum_server_url.is_empty() {
        return Ok(false);
    }

    // Try to create client if not exists
    if get_or_create_medplum_client(medplum_state.inner())
        .await
        .is_err()
    {
        return Ok(false);
    }

    let client_guard = medplum_state.read().await;
    if let Some(ref client) = *client_guard {
        // Check server connectivity (not authentication)
        Ok(client.check_server_connectivity().await)
    } else {
        Ok(false)
    }
}
