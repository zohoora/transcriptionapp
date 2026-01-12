// Shared type definitions for the Transcription App
// These types mirror the Rust backend types and are used across the frontend

// Session state types
export type SessionState =
  | 'idle'
  | 'preparing'
  | 'recording'
  | 'stopping'
  | 'completed'
  | 'error';

export interface SessionStatus {
  state: SessionState;
  provider: 'whisper' | 'apple' | null;
  elapsed_ms: number;
  is_processing_behind: boolean;
  error_message?: string;
}

export interface TranscriptUpdate {
  finalized_text: string;
  draft_text: string | null;
  segment_count: number;
}

export interface Device {
  id: string;
  name: string;
  is_default: boolean;
}

export interface ModelStatus {
  available: boolean;
  path: string | null;
  error: string | null;
}

export interface WhisperModelInfo {
  id: string;
  label: string;
  category: string;
  filename: string;
  url: string;
  size_bytes: number;
  description: string;
  downloaded: boolean;
  recommended: boolean;
  english_only: boolean;
}

export interface Settings {
  whisper_model: string;
  language: string;
  input_device_id: string | null;
  output_format: string;
  vad_threshold: number;
  silence_to_flush_ms: number;
  max_utterance_ms: number;
  diarization_enabled: boolean;
  max_speakers: number;
  // LLM Router settings (OpenAI-compatible API)
  llm_router_url: string;
  llm_api_key: string;
  llm_client_id: string;
  soap_model: string;
  fast_model: string;
  // Medplum EMR settings
  medplum_server_url: string;
  medplum_client_id: string;
  medplum_auto_sync: boolean;
  // Whisper server settings (remote transcription only - local mode removed)
  whisper_mode: 'remote';  // Always 'remote' - local mode no longer supported
  whisper_server_url: string;
  whisper_server_model: string;
  // SOAP note generation preferences (persisted)
  soap_detail_level: number;
  soap_format: SoapFormat;
  soap_custom_instructions: string;
  // Auto-session detection settings
  auto_start_enabled: boolean;
  greeting_sensitivity: number | null;
  min_speech_duration_ms: number | null;
}

// Listening mode types (auto-session detection)
export interface ListeningStatus {
  is_listening: boolean;
  speech_detected: boolean;
  speech_duration_ms: number;
  analyzing: boolean;
}

// Rust uses #[serde(tag = "type", rename_all = "snake_case")]
export type ListeningEventType =
  | 'started'
  | 'speech_detected'
  | 'analyzing'
  | 'start_recording'      // Optimistic recording start (before greeting check completes)
  | 'greeting_confirmed'   // Greeting check passed, recording should continue
  | 'greeting_rejected'    // Not a greeting, recording should be discarded
  | 'greeting_detected'    // Legacy: greeting detected
  | 'not_greeting'         // Legacy: not a greeting
  | 'error'
  | 'stopped';

export interface ListeningEventPayload {
  type: ListeningEventType;
  // Optional fields depending on event type
  duration_ms?: number;
  initial_audio_duration_ms?: number;  // For start_recording event
  transcript?: string;
  confidence?: number;
  detected_phrase?: string | null;
  message?: string;
  reason?: string;  // For greeting_rejected event
}

// Checklist types
export type CheckCategory = 'audio' | 'model' | 'permission' | 'configuration' | 'network';
export type CheckStatus = 'pass' | 'fail' | 'warning' | 'pending' | 'skipped';

export interface CheckAction {
  download_model?: { model_name: string };
  open_settings?: { settings_type: string };
  retry?: boolean;
  none?: boolean;
}

export interface CheckResult {
  id: string;
  name: string;
  category: CheckCategory;
  status: CheckStatus;
  message: string | null;
  action: CheckAction | null;
}

export interface ChecklistResult {
  checks: CheckResult[];
  all_passed: boolean;
  can_start: boolean;
  summary: string;
}

// Biomarker types
export interface CoughEvent {
  timestamp_ms: number;
  duration_ms: number;
  confidence: number;
  label: string;
}

export interface SpeakerBiomarkers {
  speaker_id: string;
  vitality_mean: number | null;
  stability_mean: number | null;
  utterance_count: number;
  talk_time_ms: number;
  turn_count: number;
  mean_turn_duration_ms: number;
  median_turn_duration_ms: number;
}

// Conversation dynamics types
export interface SpeakerTurnStats {
  speaker_id: string;
  turn_count: number;
  mean_turn_duration_ms: number;
  median_turn_duration_ms: number;
}

export interface SilenceStats {
  total_silence_ms: number;
  long_pause_count: number;
  mean_pause_duration_ms: number;
  silence_ratio: number;
}

export interface ConversationDynamics {
  speaker_turns: SpeakerTurnStats[];
  silence: SilenceStats;
  total_overlap_count: number;
  total_interruption_count: number;
  mean_response_latency_ms: number;
  engagement_score: number | null;
}

export interface BiomarkerUpdate {
  cough_count: number;
  cough_rate_per_min: number;
  turn_count: number;
  avg_turn_duration_ms: number;
  talk_time_ratio: number | null;
  vitality_session_mean: number | null;
  stability_session_mean: number | null;
  speaker_metrics: SpeakerBiomarkers[];
  recent_events: CoughEvent[];
  conversation_dynamics: ConversationDynamics | null;
}

// Audio quality types
export interface AudioQualitySnapshot {
  timestamp_ms: number;
  peak_db: number;
  rms_db: number;
  clipped_samples: number;
  clipped_ratio: number;
  noise_floor_db: number;
  snr_db: number;
  silence_ratio: number;
  dropout_count: number;
  total_clipped: number;
  total_samples: number;
}

// LLM Router / SOAP Note types
// Note: Named OllamaStatus for backward compatibility with existing code
export interface OllamaStatus {
  connected: boolean;
  available_models: string[];
  error: string | null;
}

// Type alias for clarity
export type LLMStatus = OllamaStatus;

// Whisper Server types (for remote transcription)
export interface WhisperServerStatus {
  connected: boolean;
  available_models: string[];
  error: string | null;
}

export interface SoapNote {
  subjective: string;
  objective: string;
  assessment: string;
  plan: string;
  generated_at: string;
  model_used: string;
  /** Raw response from the model (for debugging) */
  raw_response?: string;
}

/** Per-patient SOAP note with speaker identification */
export interface PatientSoapNote {
  /** Label for this patient (e.g., "Patient 1", "Patient 2") */
  patient_label: string;
  /** Which speaker this patient was identified as (e.g., "Speaker 1", "Speaker 3") */
  speaker_id: string;
  /** The SOAP note for this patient */
  soap: SoapNote;
}

/** Multi-patient SOAP result from LLM auto-detection */
export interface MultiPatientSoapResult {
  /** Individual SOAP notes for each patient detected (1-4 patients) */
  notes: PatientSoapNote[];
  /** Which speaker was identified as the physician (e.g., "Speaker 2") */
  physician_speaker: string | null;
  /** When the result was generated */
  generated_at: string;
  /** Which LLM model was used */
  model_used: string;
}

/** SOAP note format style */
export type SoapFormat = 'problem_based' | 'comprehensive';

/** Options for SOAP note generation */
export interface SoapOptions {
  /** Detail level (1-10, where 5 is standard) */
  detail_level: number;
  /** SOAP format style */
  format: SoapFormat;
  /** Custom instructions from the physician */
  custom_instructions: string;
}

/** Default SOAP options */
export const DEFAULT_SOAP_OPTIONS: SoapOptions = {
  detail_level: 5,
  format: 'problem_based',
  custom_instructions: '',
};

/** Detail level labels for display */
export const DETAIL_LEVEL_LABELS: Record<number, { name: string; description: string }> = {
  1: { name: 'Ultra-Brief', description: '1-2 bullet points, only critical info' },
  2: { name: 'Minimal', description: '2-3 bullet points, key symptoms only' },
  3: { name: 'Brief', description: '3-4 bullets, primary complaint focus' },
  4: { name: 'Short', description: 'Fewer bullets, combined items' },
  5: { name: 'Standard', description: 'Default balanced detail' },
  6: { name: 'Expanded', description: 'Additional descriptors, context' },
  7: { name: 'Detailed', description: 'Timing, severity, quality, history' },
  8: { name: 'Thorough', description: 'OPQRST, pertinent negatives, full differential' },
  9: { name: 'Comprehensive', description: 'Extensive history, ROS, patient education' },
  10: { name: 'Maximum', description: 'Every detail, nothing omitted' },
};

// Constants for biomarker interpretation
export const BIOMARKER_THRESHOLDS = {
  // Vitality: F0 std dev in Hz. Normal speech: 30-80 Hz, low vitality: <20 Hz
  VITALITY_GOOD: 30,      // Hz - above this is good
  VITALITY_WARNING: 15,   // Hz - above this is warning, below is low
  VITALITY_MAX_DISPLAY: 60, // Hz - 100% on progress bar

  // Stability: CPP in dB. Normal: 8-15 dB, concerning: <6 dB
  STABILITY_GOOD: 8,      // dB - above this is good
  STABILITY_WARNING: 5,   // dB - above this is warning, below is low
  STABILITY_MAX_DISPLAY: 12, // dB - 100% on progress bar

  // Response latency thresholds
  RESPONSE_LATENCY_GOOD: 500,     // ms - below this is good
  RESPONSE_LATENCY_WARNING: 1500, // ms - below this is warning, above is slow

  // Engagement score thresholds
  ENGAGEMENT_GOOD: 70,    // 0-100 - above this is good
  ENGAGEMENT_WARNING: 40, // 0-100 - above this is warning, below is low
} as const;

export const AUDIO_QUALITY_THRESHOLDS = {
  // RMS level in dBFS
  LEVEL_TOO_QUIET: -40,   // dBFS - below this is too quiet
  LEVEL_TOO_HOT: -6,      // dBFS - above this is too hot
  LEVEL_MIN_DISPLAY: -60, // dBFS - 0% on progress bar
  LEVEL_MAX_DISPLAY: 0,   // dBFS - 100% on progress bar

  // SNR in dB
  SNR_GOOD: 15,           // dB - above this is good
  SNR_WARNING: 10,        // dB - above this is warning, below is poor
  SNR_MAX_DISPLAY: 30,    // dB - 100% on progress bar

  // Clipping ratio
  CLIPPING_OK: 0.001,     // 0.1% - below this is acceptable
} as const;

// ============================================================================
// Medplum EMR Types
// ============================================================================

export interface AuthState {
  is_authenticated: boolean;
  access_token: string | null;
  refresh_token: string | null;
  token_expiry: number | null;
  practitioner_id: string | null;
  practitioner_name: string | null;
}

export interface AuthUrl {
  url: string;
  state: string;
}

export interface Patient {
  id: string;
  name: string;
  mrn: string | null;
  birthDate: string | null;
}

export interface Encounter {
  id: string;
  patientId: string;
  patientName: string;
  status: 'in-progress' | 'finished' | 'cancelled';
  startTime: string;
  endTime: string | null;
}

export interface EncounterSummary {
  id: string;
  fhirId: string;
  patientName: string;
  date: string;
  durationMinutes: number | null;
  hasSoapNote: boolean;
  hasAudio: boolean;
}

export interface EncounterDetails extends EncounterSummary {
  transcript: string | null;
  soapNote: string | null;
  audioUrl: string | null;
  sessionInfo: string | null;
}

export interface SyncStatus {
  encounterSynced: boolean;
  transcriptSynced: boolean;
  soapNoteSynced: boolean;
  audioSynced: boolean;
  lastSyncTime: string | null;
}

export interface SyncResult {
  success: boolean;
  status: SyncStatus;
  error: string | null;
  /** Local encounter UUID for log correlation */
  encounterId?: string;
  /** FHIR server-assigned encounter ID for updates */
  encounterFhirId?: string;
}

/** Tracks a synced encounter for subsequent updates */
export interface SyncedEncounter {
  /** Local encounter UUID */
  encounterId: string;
  /** FHIR server-assigned encounter ID */
  encounterFhirId: string;
  /** When the initial sync occurred */
  syncedAt: string;
  /** Whether SOAP note has been synced */
  hasSoap: boolean;
}

/** Info about a synced patient in multi-patient sync */
export interface PatientSyncInfo {
  /** Label from SOAP result (e.g., "Patient 1") */
  patientLabel: string;
  /** Speaker ID from transcript (e.g., "Speaker 1") */
  speakerId: string;
  /** Created patient's FHIR ID */
  patientFhirId: string;
  /** Created encounter's FHIR ID */
  encounterFhirId: string;
  /** Whether SOAP note was synced */
  hasSoap: boolean;
}

/** Result of multi-patient sync operation */
export interface MultiPatientSyncResult {
  /** Whether all syncs succeeded */
  success: boolean;
  /** Info about each synced patient/encounter */
  patients: PatientSyncInfo[];
  /** Error message if any patient sync failed */
  error: string | null;
}
