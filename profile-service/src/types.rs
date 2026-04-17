use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Physician ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicianProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specialty: Option<String>,

    // Physician-tier settings (None = use room default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_detail_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_custom_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charting_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_require_enrolled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_required_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_end_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_end_silence_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_merge_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_check_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_silence_trigger_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_auto_sync: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diarization_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_speakers: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_practitioner_id: Option<String>,

    pub created_at: String,
    pub updated_at: String,
}

/// Request body for creating a physician (id + timestamps are server-generated)
#[derive(Debug, Deserialize)]
pub struct CreatePhysicianRequest {
    pub name: String,
    #[serde(default)]
    pub specialty: Option<String>,
}

impl CreatePhysicianRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.name.is_empty() {
            return Err(ApiError::BadRequest("Name must not be empty".into()));
        }
        if self.name.len() > 500 {
            return Err(ApiError::BadRequest("Name exceeds 500 characters".into()));
        }
        if self.specialty.as_ref().map_or(false, |s| s.len() > 500) {
            return Err(ApiError::BadRequest(
                "Specialty exceeds 500 characters".into(),
            ));
        }
        Ok(())
    }
}

/// Request body for partial update of physician preferences
#[derive(Debug, Deserialize)]
pub struct UpdatePhysicianRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub specialty: Option<String>,
    #[serde(default)]
    pub soap_detail_level: Option<u8>,
    #[serde(default)]
    pub soap_format: Option<String>,
    #[serde(default)]
    pub soap_custom_instructions: Option<String>,
    #[serde(default)]
    pub charting_mode: Option<String>,
    #[serde(default)]
    pub image_source: Option<String>,
    #[serde(default)]
    pub gemini_api_key: Option<String>,
    #[serde(default)]
    pub auto_start_enabled: Option<bool>,
    #[serde(default)]
    pub auto_start_require_enrolled: Option<bool>,
    #[serde(default)]
    pub auto_start_required_role: Option<String>,
    #[serde(default)]
    pub auto_end_enabled: Option<bool>,
    #[serde(default)]
    pub auto_end_silence_ms: Option<u64>,
    #[serde(default)]
    pub encounter_merge_enabled: Option<bool>,
    #[serde(default)]
    pub encounter_check_interval_secs: Option<u32>,
    #[serde(default)]
    pub encounter_silence_trigger_secs: Option<u32>,
    #[serde(default)]
    pub medplum_auto_sync: Option<bool>,
    #[serde(default)]
    pub diarization_enabled: Option<bool>,
    #[serde(default)]
    pub max_speakers: Option<usize>,
    #[serde(default)]
    pub medplum_practitioner_id: Option<String>,
}

impl UpdatePhysicianRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if let Some(ref name) = self.name {
            if name.is_empty() {
                return Err(ApiError::BadRequest("Name must not be empty".into()));
            }
            if name.len() > 500 {
                return Err(ApiError::BadRequest("Name exceeds 500 characters".into()));
            }
        }
        if self.specialty.as_ref().map_or(false, |s| s.len() > 500) {
            return Err(ApiError::BadRequest(
                "Specialty exceeds 500 characters".into(),
            ));
        }
        if self
            .soap_custom_instructions
            .as_ref()
            .map_or(false, |s| s.len() > 10_000)
        {
            return Err(ApiError::BadRequest(
                "Custom instructions exceed 10000 characters".into(),
            ));
        }
        if self.soap_format.as_ref().map_or(false, |s| s.len() > 500) {
            return Err(ApiError::BadRequest(
                "SOAP format exceeds 500 characters".into(),
            ));
        }
        if self.charting_mode.as_ref().map_or(false, |s| s.len() > 100) {
            return Err(ApiError::BadRequest(
                "Charting mode exceeds 100 characters".into(),
            ));
        }
        Ok(())
    }
}

// ── Infrastructure ────────────────────────────────────────────────

/// Clinic-wide infrastructure settings (singleton — one per deployment).
/// All fields are `Option` so partial updates merge cleanly.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InfrastructureSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_router_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_model_fast: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_server_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_postprocess: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miis_server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_detection_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_detection_nothink: Option<bool>,
}

/// Request body for partial update of infrastructure settings
#[derive(Debug, Deserialize)]
pub struct UpdateInfrastructureRequest {
    #[serde(default)]
    pub llm_router_url: Option<String>,
    #[serde(default)]
    pub llm_api_key: Option<String>,
    #[serde(default)]
    pub llm_client_id: Option<String>,
    #[serde(default)]
    pub soap_model: Option<String>,
    #[serde(default)]
    pub soap_model_fast: Option<String>,
    #[serde(default)]
    pub fast_model: Option<String>,
    #[serde(default)]
    pub whisper_server_url: Option<String>,
    #[serde(default)]
    pub whisper_server_model: Option<String>,
    #[serde(default)]
    pub stt_alias: Option<String>,
    #[serde(default)]
    pub stt_postprocess: Option<bool>,
    #[serde(default)]
    pub medplum_server_url: Option<String>,
    #[serde(default)]
    pub medplum_client_id: Option<String>,
    #[serde(default)]
    pub miis_server_url: Option<String>,
    #[serde(default)]
    pub whisper_mode: Option<String>,
    #[serde(default)]
    pub encounter_detection_model: Option<String>,
    #[serde(default)]
    pub encounter_detection_nothink: Option<bool>,
}

// ── Room ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // Room-tier settings (None = use infrastructure default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_detection_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_sensor_port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_sensor_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_absence_threshold_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_debounce_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_hot_pixel_threshold_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub co2_baseline_ppm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid_confirm_window_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid_min_words_for_sensor_split: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_capture_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_capture_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_active_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_csv_log_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_csv_log_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vad_threshold: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silence_to_flush_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_utterance_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greeting_sensitivity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_speech_duration_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_storage_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device_id: Option<String>,

    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
    pub description: Option<String>,
}

impl CreateRoomRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.name.is_empty() {
            return Err(ApiError::BadRequest("Name must not be empty".into()));
        }
        if self.name.len() > 500 {
            return Err(ApiError::BadRequest("Name exceeds 500 characters".into()));
        }
        if self.description.as_ref().map_or(false, |s| s.len() > 2000) {
            return Err(ApiError::BadRequest(
                "Description exceeds 2000 characters".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoomRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    // Room-tier settings
    #[serde(default)]
    pub encounter_detection_mode: Option<String>,
    #[serde(default)]
    pub presence_sensor_port: Option<String>,
    #[serde(default)]
    pub presence_sensor_url: Option<String>,
    #[serde(default)]
    pub presence_absence_threshold_secs: Option<u64>,
    #[serde(default)]
    pub presence_debounce_secs: Option<u64>,
    #[serde(default)]
    pub thermal_hot_pixel_threshold_c: Option<f32>,
    #[serde(default)]
    pub co2_baseline_ppm: Option<f32>,
    #[serde(default)]
    pub hybrid_confirm_window_secs: Option<u64>,
    #[serde(default)]
    pub hybrid_min_words_for_sensor_split: Option<usize>,
    #[serde(default)]
    pub screen_capture_enabled: Option<bool>,
    #[serde(default)]
    pub screen_capture_interval_secs: Option<u32>,
    #[serde(default)]
    pub shadow_active_method: Option<String>,
    #[serde(default)]
    pub shadow_csv_log_enabled: Option<bool>,
    #[serde(default)]
    pub presence_csv_log_enabled: Option<bool>,
    #[serde(default)]
    pub vad_threshold: Option<f32>,
    #[serde(default)]
    pub silence_to_flush_ms: Option<u32>,
    #[serde(default)]
    pub max_utterance_ms: Option<u32>,
    #[serde(default)]
    pub greeting_sensitivity: Option<f32>,
    #[serde(default)]
    pub min_speech_duration_ms: Option<u32>,
    #[serde(default)]
    pub whisper_model: Option<String>,
    #[serde(default)]
    pub debug_storage_enabled: Option<bool>,
    #[serde(default)]
    pub input_device_id: Option<String>,
}

impl UpdateRoomRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if let Some(ref name) = self.name {
            if name.is_empty() {
                return Err(ApiError::BadRequest("Name must not be empty".into()));
            }
            if name.len() > 500 {
                return Err(ApiError::BadRequest("Name exceeds 500 characters".into()));
            }
        }
        if self.description.as_ref().map_or(false, |s| s.len() > 2000) {
            return Err(ApiError::BadRequest(
                "Description exceeds 2000 characters".into(),
            ));
        }
        Ok(())
    }
}

// ── Speaker ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerRole {
    Physician,
    Pa,
    Rn,
    Ma,
    Patient,
    Other,
}

impl Default for SpeakerRole {
    fn default() -> Self {
        SpeakerRole::Other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerProfile {
    pub id: String,
    pub name: String,
    pub role: SpeakerRole,
    pub description: String,
    pub embedding: Vec<f32>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateSpeakerRequest {
    pub name: String,
    #[serde(default)]
    pub role: SpeakerRole,
    #[serde(default)]
    pub description: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpeakerRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<SpeakerRole>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

// ── Session ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveMetadata {
    pub session_id: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub segment_count: usize,
    #[serde(default)]
    pub word_count: usize,
    #[serde(default)]
    pub has_soap_note: bool,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default)]
    pub auto_ended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_end_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_detail_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charting_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detection_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub likely_non_clinical: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_count: Option<u32>,
    // Multi-user fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physician_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physician_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_patient_handout: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_billing_record: Option<bool>,
    /// Vision-extracted patient date of birth (YYYY-MM-DD). Populated during
    /// continuous mode when the visible EMR chart header yields a DOB via the
    /// `vision-model` call. Used by the tauri billing context to auto-derive
    /// the patient age bracket for OHIP codes with age-gated fees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_dob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSummary {
    pub session_id: String,
    pub date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub word_count: usize,
    #[serde(default)]
    pub has_soap_note: bool,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default)]
    pub auto_ended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charting_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub likely_non_clinical: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_feedback: Option<bool>,
    // Multi-user fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physician_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_billing_record: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedPatientNote {
    pub index: u32,
    pub label: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveDetails {
    pub session_id: String,
    pub metadata: ArchiveMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_path: Option<String>,
    #[serde(default, rename = "patientNotes", skip_serializing_if = "Option::is_none")]
    pub patient_notes: Option<Vec<ArchivedPatientNote>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFeedback {
    pub schema_version: u32,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_rating: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detection_feedback: Option<DetectionFeedback>,
    #[serde(default)]
    pub patient_feedback: Vec<PatientContentFeedback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionFeedback {
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientContentFeedback {
    pub patient_index: usize,
    pub issues: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Request body for uploading a session
#[derive(Debug, Deserialize)]
pub struct UploadSessionRequest {
    pub metadata: ArchiveMetadata,
    pub transcript: String,
    #[serde(default)]
    pub soap_note: Option<String>,
}

/// Request body for updating SOAP
#[derive(Debug, Deserialize)]
pub struct UpdateSoapRequest {
    pub content: String,
    #[serde(default)]
    pub detail_level: Option<u8>,
    #[serde(default)]
    pub format: Option<String>,
}

/// Request body for updating patient name
#[derive(Debug, Deserialize)]
pub struct UpdatePatientNameRequest {
    pub patient_name: String,
}

/// Request body for splitting a session
#[derive(Debug, Deserialize)]
pub struct SplitSessionRequest {
    pub split_line: usize,
}

/// Request body for merging sessions
#[derive(Debug, Deserialize)]
pub struct MergeSessionsRequest {
    pub session_ids: Vec<String>,
    pub date: String,
}

/// Request body for renumbering encounters
#[derive(Debug, Deserialize)]
pub struct RenumberRequest {
    pub date: String,
}

// ── Server-Configurable Data ─────────────────────────────────────

/// Version metadata for the config data bundle.
/// Clients compare their cached version against this to detect staleness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    pub version: u64,
    pub updated_at: String,
}

// ── Prompt Templates ─────────────────────────────────────────────

/// All LLM prompt templates served to clients.
/// The client fetches this at startup and uses the templates instead of
/// compiled-in string literals. Dynamic variables use `{variable_name}` syntax.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplates {
    #[serde(default)]
    pub version: u64,

    // SOAP note generation — stored as fragments for flexible assembly.
    // The client assembles: base_template + detail_instruction + format_instruction + custom_section.
    /// Main SOAP system prompt body. Placeholders: {custom_section}, {detail_instruction}, {format_instruction}
    #[serde(default)]
    pub soap_base_template: String,
    /// Detail instruction per level range: keys "1-3", "4-6", "7-10". Placeholder: {detail_level}
    #[serde(default)]
    pub soap_detail_instructions: HashMap<String, String>,
    /// Format instruction per SOAP format: keys "problem_based", "comprehensive"
    #[serde(default)]
    pub soap_format_instructions: HashMap<String, String>,
    /// Custom instruction section templates: keys "global_only", "session_only", "both"
    /// Each has placeholders like {global_instructions}, {session_instructions}
    #[serde(default)]
    pub soap_custom_section_templates: HashMap<String, String>,
    /// Vision-based SOAP system prompt (step-by-step verification, no dynamic vars)
    #[serde(default)]
    pub soap_vision_template: String,
    /// Extension text appended to base prompt for per-patient SOAP (multi-patient context)
    #[serde(default)]
    pub soap_per_patient_extension: String,
    /// Extension text for scoping SOAP to a single patient label. Placeholder: {patient_label}
    #[serde(default)]
    pub soap_single_patient_scope_template: String,

    // Patient handout
    /// Patient handout generation system prompt (5th-8th grade reading level, plain text)
    #[serde(default)]
    pub patient_handout: String,

    // Encounter detection
    /// Encounter detection system prompt (transition-point detection framing)
    #[serde(default)]
    pub encounter_detection_system: String,
    /// Context text when sensor detects departure (soft signal framing)
    #[serde(default)]
    pub encounter_detection_sensor_departed: String,
    /// Context text when sensor confirms presence (conservative "NOT transitions" framing)
    #[serde(default)]
    pub encounter_detection_sensor_present: String,

    // Clinical content check
    /// Binary clinical-vs-non-clinical classification system prompt
    #[serde(default)]
    pub clinical_content_check: String,

    // Multi-patient prompts (all static, no dynamic vars)
    /// Gate check after merge-back: confirms multiple patients vs companions
    #[serde(default)]
    pub multi_patient_check: String,
    /// Structured patient extraction (count, labels, summaries)
    #[serde(default)]
    pub multi_patient_detect: String,
    /// Enhanced split-point detection via name transitions
    #[serde(default)]
    pub multi_patient_split: String,

    // Encounter merge
    /// Merge check system prompt. Optional placeholder: {patient_context}
    #[serde(default)]
    pub encounter_merge_system: String,

    // Patient name/DOB extraction
    /// Vision model system prompt for extracting patient name from screenshots
    #[serde(default)]
    pub patient_name_system: String,
    /// Vision model user prompt (JSON schema instruction)
    #[serde(default)]
    pub patient_name_user: String,

    // Greeting detection
    /// Greeting detection system prompt. Placeholder: {sensitivity}
    #[serde(default)]
    pub greeting_detection: String,

    // Billing extraction
    /// Billing feature extraction system prompt (large schema: visit types, procedures, conditions)
    #[serde(default)]
    pub billing_extraction: String,

    // Patient merge correction
    /// Physician-reviewed multi-patient merge correction prompt
    #[serde(default)]
    pub patient_merge_correction: String,
}

impl Default for PromptTemplates {
    fn default() -> Self {
        Self {
            version: 0,
            soap_base_template: String::new(),
            soap_detail_instructions: HashMap::new(),
            soap_format_instructions: HashMap::new(),
            soap_custom_section_templates: HashMap::new(),
            soap_vision_template: String::new(),
            soap_per_patient_extension: String::new(),
            soap_single_patient_scope_template: String::new(),
            patient_handout: String::new(),
            encounter_detection_system: String::new(),
            encounter_detection_sensor_departed: String::new(),
            encounter_detection_sensor_present: String::new(),
            clinical_content_check: String::new(),
            multi_patient_check: String::new(),
            multi_patient_detect: String::new(),
            multi_patient_split: String::new(),
            encounter_merge_system: String::new(),
            patient_name_system: String::new(),
            patient_name_user: String::new(),
            greeting_detection: String::new(),
            billing_extraction: String::new(),
            patient_merge_correction: String::new(),
        }
    }
}

// ── Billing Data ─────────────────────────────────────────────────

/// Server-configurable billing data: OHIP codes, diagnostic codes,
/// exclusion groups, and mapping tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingData {
    #[serde(default)]
    pub version: u64,
    /// OHIP fee schedule codes (236 codes as of Apr 2026 SOB)
    #[serde(default)]
    pub ohip_codes: Vec<OhipCodeEntry>,
    /// ICD-8 diagnostic codes (562 codes as of MOH Apr 2023 + Mar 2026)
    #[serde(default)]
    pub diagnostic_codes: Vec<DiagnosticCodeEntry>,
    /// Code incompatibility groups (21 groups)
    #[serde(default)]
    pub exclusion_groups: Vec<ExclusionGroupEntry>,
    /// VisitType enum variant → OHIP assessment code mapping
    #[serde(default)]
    pub visit_type_mappings: HashMap<String, VisitTypeMappingEntry>,
    /// ProcedureType enum variant → OHIP procedure code mapping
    #[serde(default)]
    pub procedure_type_mappings: HashMap<String, String>,
    /// ConditionType enum variant → list of K/Q codes
    #[serde(default)]
    pub condition_type_mappings: HashMap<String, Vec<String>>,
    /// Companion code auto-add rules (e.g., tray fee with procedures)
    #[serde(default)]
    pub companion_rules: Vec<CompanionRule>,
    /// Time-based billing rates (Q310A, Q311A)
    #[serde(default)]
    pub time_rates: Vec<TimeRate>,
    /// Counselling duration thresholds in minutes for K013A/K033A units
    #[serde(default)]
    pub counselling_unit_thresholds: Vec<u64>,
    /// Billing code → implied diagnostic code mapping (e.g., K030A → 250)
    #[serde(default)]
    pub code_implied_diagnostics: HashMap<String, String>,
    /// Codes that qualify for tray fee (E542A) companion
    #[serde(default)]
    pub tray_fee_qualifying_codes: Vec<String>,
}

impl Default for BillingData {
    fn default() -> Self {
        Self {
            version: 0,
            ohip_codes: Vec::new(),
            diagnostic_codes: Vec::new(),
            exclusion_groups: Vec::new(),
            visit_type_mappings: HashMap::new(),
            procedure_type_mappings: HashMap::new(),
            condition_type_mappings: HashMap::new(),
            companion_rules: Vec::new(),
            time_rates: Vec::new(),
            counselling_unit_thresholds: Vec::new(),
            code_implied_diagnostics: HashMap::new(),
            tray_fee_qualifying_codes: Vec::new(),
        }
    }
}

/// A single OHIP billing code entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OhipCodeEntry {
    pub code: String,
    pub description: String,
    pub ffs_rate_cents: u32,
    /// "in" or "out" (in-basket vs out-of-basket)
    pub basket: String,
    pub shadow_pct: u8,
    /// Category: "assessment", "counselling", "procedure", "chronic_disease",
    /// "screening", "premium", "time_based", "immunization"
    pub category: String,
    pub after_hours_eligible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_per_year: Option<u8>,
}

/// A single ICD-8 diagnostic code entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCodeEntry {
    pub code: String,
    pub description: String,
    pub category: String,
}

/// Code exclusion group — codes that cannot be billed together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExclusionGroupEntry {
    pub name: String,
    pub codes: Vec<String>,
    #[serde(default)]
    pub reason: String,
}

/// Visit type mapping: maps a VisitType enum variant to an OHIP code
/// with optional quantity and alternative code logic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitTypeMappingEntry {
    pub code: String,
    /// For counselling types, the quantity is computed from duration
    #[serde(default)]
    pub quantity_from_duration: bool,
    /// Alternative code when counselling is exhausted (e.g., K013A → K033A)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhausted_alternative: Option<String>,
}

/// Companion code auto-add rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionRule {
    /// Code that triggers the companion (e.g., "G365A" for pap tray)
    pub trigger_code: String,
    /// Companion code to add (e.g., "E430A")
    pub companion_code: String,
    /// Condition: "not_hospital", "always"
    #[serde(default)]
    pub condition: String,
}

/// Time-based billing rate entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRate {
    pub code: String,
    pub description: String,
    pub rate_per_15min_cents: u32,
    /// Encounter settings this rate applies to
    pub settings: Vec<String>,
}

// ── Detection Thresholds ─────────────────────────────────────────

/// Server-configurable encounter detection thresholds and pipeline constants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionThresholds {
    #[serde(default)]
    pub version: u64,

    // ── Encounter detection word thresholds ──
    /// Force encounter check regardless of timer when buffer exceeds this (default: 3000)
    #[serde(default = "default_3000")]
    pub force_check_word_threshold: usize,
    /// Word count for graduated force-split (default: 5000)
    #[serde(default = "default_5000")]
    pub force_split_word_threshold: usize,
    /// Consecutive LLM failures before force-split (default: 3)
    #[serde(default = "default_3")]
    pub force_split_consecutive_limit: u32,
    /// Unconditional force-split safety valve (default: 25000)
    #[serde(default = "default_25000")]
    pub absolute_word_cap: usize,
    /// Minimum words for clinical content check (default: 100)
    #[serde(default = "default_100")]
    pub min_words_for_clinical_check: usize,
    /// Grace period after split for stale screenshot suppression in seconds (default: 90)
    #[serde(default = "default_90")]
    pub screenshot_stale_grace_secs: i64,
    /// Minimum merged words for retrospective multi-patient check (default: 2500)
    #[serde(default = "default_2500")]
    pub multi_patient_check_word_threshold: usize,
    /// Minimum words per half for retrospective split acceptance (default: 500)
    #[serde(default = "default_500_usize")]
    pub multi_patient_split_min_words: usize,

    // ── Confidence thresholds ──
    /// Base confidence for short encounters (<threshold mins) (default: 0.85)
    #[serde(default = "default_085")]
    pub confidence_base_short: f64,
    /// Base confidence for long encounters (>=threshold mins) (default: 0.70)
    #[serde(default = "default_070")]
    pub confidence_base_long: f64,
    /// Buffer age in minutes that separates short/long threshold (default: 20)
    #[serde(default = "default_20")]
    pub confidence_age_threshold_mins: i64,
    /// Confidence increment per merge-back (default: 0.05)
    #[serde(default = "default_005")]
    pub confidence_merge_back_increment: f64,
    /// Maximum confidence threshold (default: 0.99)
    #[serde(default = "default_099")]
    pub confidence_max: f64,

    // ── Pipeline timeouts ──
    /// SOAP generation LLM timeout in seconds (default: 300)
    #[serde(default = "default_300")]
    pub soap_generation_timeout_secs: u64,
    /// Billing extraction LLM timeout in seconds (default: 300)
    #[serde(default = "default_300")]
    pub billing_extraction_timeout_secs: u64,

    // ── Continuous mode constants ──
    /// Words to use for merge check excerpt (default: 500)
    #[serde(default = "default_500_usize")]
    pub merge_excerpt_words: usize,
    /// Max words to skip clinical check (idle speech) (default: 200)
    #[serde(default = "default_200")]
    pub idle_encounter_max_words: usize,
    /// Minimum accepted split size in words (default: 100)
    #[serde(default = "default_100")]
    pub min_split_word_floor: usize,

    // ── Billing cap constants ──
    /// Daily direct care hour limit (default: 14.0)
    #[serde(default = "default_14f")]
    pub daily_hour_limit: f32,
    /// Monthly (28-day rolling) hour limit (default: 240.0)
    #[serde(default = "default_240f")]
    pub monthly_hour_limit: f32,
    /// Monthly rolling window in days (default: 28)
    #[serde(default = "default_28")]
    pub monthly_window_days: u32,

    // ── Category A extensions (Phase 3) ──
    /// Min merged words before retrospective multi-patient detection runs (default: 500)
    #[serde(default = "default_mp_detect_word_threshold")]
    pub multi_patient_detect_word_threshold: usize,
    /// Consecutive matching vision votes to early-stop the vision LLM call (default: 5)
    #[serde(default = "default_vision_skip_streak_k")]
    pub vision_skip_streak_k: usize,
    /// Pathological backstop cap on vision calls per encounter (default: 30)
    #[serde(default = "default_vision_skip_call_cap")]
    pub vision_skip_call_cap: usize,
    /// Gemini image generation HTTP timeout in seconds (default: 45)
    #[serde(default = "default_gemini_generation_timeout_secs")]
    pub gemini_generation_timeout_secs: u64,
}

// Serde default value functions for DetectionThresholds
fn default_3000() -> usize { 3000 }
fn default_5000() -> usize { 5000 }
fn default_3() -> u32 { 3 }
fn default_25000() -> usize { 25000 }
fn default_100() -> usize { 100 }
fn default_90() -> i64 { 90 }
fn default_2500() -> usize { 2500 }
fn default_500_usize() -> usize { 500 }
fn default_085() -> f64 { 0.85 }
fn default_070() -> f64 { 0.70 }
fn default_20() -> i64 { 20 }
fn default_005() -> f64 { 0.05 }
fn default_099() -> f64 { 0.99 }
fn default_300() -> u64 { 300 }
fn default_200() -> usize { 200 }
fn default_14f() -> f32 { 14.0 }
fn default_240f() -> f32 { 240.0 }
fn default_28() -> u32 { 28 }
fn default_mp_detect_word_threshold() -> usize { 500 }
fn default_vision_skip_streak_k() -> usize { 5 }
fn default_vision_skip_call_cap() -> usize { 30 }
fn default_gemini_generation_timeout_secs() -> u64 { 45 }

impl Default for DetectionThresholds {
    fn default() -> Self {
        Self {
            version: 0,
            force_check_word_threshold: 3000,
            force_split_word_threshold: 5000,
            force_split_consecutive_limit: 3,
            absolute_word_cap: 25000,
            min_words_for_clinical_check: 100,
            screenshot_stale_grace_secs: 90,
            multi_patient_check_word_threshold: 2500,
            multi_patient_split_min_words: 500,
            confidence_base_short: 0.85,
            confidence_base_long: 0.70,
            confidence_age_threshold_mins: 20,
            confidence_merge_back_increment: 0.05,
            confidence_max: 0.99,
            soap_generation_timeout_secs: 300,
            billing_extraction_timeout_secs: 300,
            merge_excerpt_words: 500,
            idle_encounter_max_words: 200,
            min_split_word_floor: 100,
            daily_hour_limit: 14.0,
            monthly_hour_limit: 240.0,
            monthly_window_days: 28,
            multi_patient_detect_word_threshold: 500,
            vision_skip_streak_k: 5,
            vision_skip_call_cap: 30,
            gemini_generation_timeout_secs: 45,
        }
    }
}

// ── Operational Defaults ─────────────────────────────────────────

/// Server-configurable operational defaults (Phase 3 of ADR-0023).
///
/// Category B settings: operator-facing workflow knobs (sleep hours, sensor
/// baselines, encounter timing, LLM model aliases). Separate from
/// `DetectionThresholds` (algorithm internals) so admin UIs can present them
/// distinctly. Clients apply a precedence model
/// (compiled default < server value < local override if user-edited).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationalDefaults {
    #[serde(default)]
    pub version: u64,

    // ── Sleep mode (continuous mode pause hours, EST/EDT) ──
    /// Hour of day (0-23, local EST/EDT) when continuous mode auto-pauses (default: 22)
    #[serde(default = "default_sleep_start_hour")]
    pub sleep_start_hour: u8,
    /// Hour of day (0-23, local EST/EDT) when continuous mode auto-resumes (default: 6)
    #[serde(default = "default_sleep_end_hour")]
    pub sleep_end_hour: u8,

    // ── Sensor baselines (per-room calibration, Phase 2 roadmap) ──
    /// Thermal hot-pixel threshold in Celsius (default: 28.0, valid 20.0-40.0)
    #[serde(default = "default_thermal_hot_pixel_threshold_c")]
    pub thermal_hot_pixel_threshold_c: f32,
    /// CO2 ambient baseline in ppm (default: 420.0, valid 300.0-600.0)
    #[serde(default = "default_co2_baseline_ppm")]
    pub co2_baseline_ppm: f32,

    // ── Encounter detection timing ──
    /// Interval between encounter detection checks in seconds (default: 120)
    #[serde(default = "default_encounter_check_interval_secs")]
    pub encounter_check_interval_secs: u32,
    /// Silence duration that triggers an encounter-end check in seconds (default: 45)
    #[serde(default = "default_encounter_silence_trigger_secs")]
    pub encounter_silence_trigger_secs: u32,

    // ── LLM model aliases ──
    /// Alias for SOAP generation model (default: "soap-model-fast")
    #[serde(default = "default_soap_model")]
    pub soap_model: String,
    /// Alias for fast SOAP generation / patient handout model (default: "soap-model-fast")
    #[serde(default = "default_soap_model_fast")]
    pub soap_model_fast: String,
    /// Alias for fast utility model (clinical check, merge, greeting, etc.) (default: "fast-model")
    #[serde(default = "default_fast_model")]
    pub fast_model: String,
    /// Alias for encounter detection model (default: "fast-model")
    #[serde(default = "default_encounter_detection_model")]
    pub encounter_detection_model: String,
}

// Serde default value functions for OperationalDefaults
fn default_sleep_start_hour() -> u8 { 22 }
fn default_sleep_end_hour() -> u8 { 6 }
fn default_thermal_hot_pixel_threshold_c() -> f32 { 28.0 }
fn default_co2_baseline_ppm() -> f32 { 420.0 }
fn default_encounter_check_interval_secs() -> u32 { 120 }
fn default_encounter_silence_trigger_secs() -> u32 { 45 }
fn default_soap_model() -> String { "soap-model-fast".to_string() }
fn default_soap_model_fast() -> String { "soap-model-fast".to_string() }
fn default_fast_model() -> String { "fast-model".to_string() }
fn default_encounter_detection_model() -> String { "fast-model".to_string() }

impl Default for OperationalDefaults {
    fn default() -> Self {
        Self {
            version: 0,
            sleep_start_hour: default_sleep_start_hour(),
            sleep_end_hour: default_sleep_end_hour(),
            thermal_hot_pixel_threshold_c: default_thermal_hot_pixel_threshold_c(),
            co2_baseline_ppm: default_co2_baseline_ppm(),
            encounter_check_interval_secs: default_encounter_check_interval_secs(),
            encounter_silence_trigger_secs: default_encounter_silence_trigger_secs(),
            soap_model: default_soap_model(),
            soap_model_fast: default_soap_model_fast(),
            fast_model: default_fast_model(),
            encounter_detection_model: default_encounter_detection_model(),
        }
    }
}

impl OperationalDefaults {
    /// Bounds-check all fields. Returns `ApiError::BadRequest` on violation so
    /// the route layer surfaces a 400 directly (matches sibling validators).
    /// A bad PUT should 400, not propagate garbage to every workstation.
    pub fn validate(&self) -> Result<(), ApiError> {
        // NOTE: `sleep_start_hour`/`sleep_end_hour` are `u8`, so values >255 can't
        // be represented on the wire. `> 23` still catches every invalid value
        // (24..=255) and remains readable, so we keep the explicit bounds check.
        if self.sleep_start_hour > 23 {
            return Err(ApiError::BadRequest(format!(
                "sleep_start_hour must be in [0, 23], got {}",
                self.sleep_start_hour
            )));
        }
        if self.sleep_end_hour > 23 {
            return Err(ApiError::BadRequest(format!(
                "sleep_end_hour must be in [0, 23], got {}",
                self.sleep_end_hour
            )));
        }
        if self.sleep_start_hour == self.sleep_end_hour {
            return Err(ApiError::BadRequest(
                "sleep_start_hour and sleep_end_hour must differ (got equal values)".to_string(),
            ));
        }
        if !(20.0..=40.0).contains(&self.thermal_hot_pixel_threshold_c) {
            return Err(ApiError::BadRequest(format!(
                "thermal_hot_pixel_threshold_c must be in [20.0, 40.0], got {}",
                self.thermal_hot_pixel_threshold_c
            )));
        }
        if !(300.0..=600.0).contains(&self.co2_baseline_ppm) {
            return Err(ApiError::BadRequest(format!(
                "co2_baseline_ppm must be in [300.0, 600.0], got {}",
                self.co2_baseline_ppm
            )));
        }
        if !(10..=3600).contains(&self.encounter_check_interval_secs) {
            return Err(ApiError::BadRequest(format!(
                "encounter_check_interval_secs must be in [10, 3600], got {}",
                self.encounter_check_interval_secs
            )));
        }
        if !(5..=600).contains(&self.encounter_silence_trigger_secs) {
            return Err(ApiError::BadRequest(format!(
                "encounter_silence_trigger_secs must be in [5, 600], got {}",
                self.encounter_silence_trigger_secs
            )));
        }
        if self.soap_model.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "soap_model must not be empty".to_string(),
            ));
        }
        if self.soap_model_fast.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "soap_model_fast must not be empty".to_string(),
            ));
        }
        if self.fast_model.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "fast_model must not be empty".to_string(),
            ));
        }
        if self.encounter_detection_model.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "encounter_detection_model must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard: `patient_dob` was added to ArchiveMetadata in v0.10.34
    /// so it survives the typed `PUT /sessions/:id/metadata` upload path (not just
    /// the JSON-merge patch path). Keep this test so future struct reshuffles
    /// don't silently drop the field.
    #[test]
    fn archive_metadata_round_trips_patient_dob() {
        let src = r#"{
            "session_id": "abc",
            "started_at": "2026-04-15T10:00:00Z",
            "patient_dob": "1980-03-22"
        }"#;
        let parsed: ArchiveMetadata = serde_json::from_str(src).expect("parses");
        assert_eq!(parsed.patient_dob.as_deref(), Some("1980-03-22"));

        let round_tripped = serde_json::to_string(&parsed).expect("serializes");
        assert!(
            round_tripped.contains(r#""patient_dob":"1980-03-22""#),
            "patient_dob was dropped during re-serialization: {round_tripped}"
        );
    }

    /// `patient_dob` must remain fully optional so old metadata.json files
    /// written before the field existed still load.
    #[test]
    fn archive_metadata_defaults_patient_dob_to_none() {
        let src = r#"{"session_id": "abc", "started_at": "2026-04-15T10:00:00Z"}"#;
        let parsed: ArchiveMetadata = serde_json::from_str(src).expect("parses");
        assert!(parsed.patient_dob.is_none());
    }
}
