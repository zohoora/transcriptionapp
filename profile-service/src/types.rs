use serde::{Deserialize, Serialize};

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
    pub language: Option<String>,
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
    pub language: Option<String>,
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
