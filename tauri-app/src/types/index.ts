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
  session_id?: string;
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
  // Speaker verification for auto-start
  auto_start_require_enrolled: boolean;
  auto_start_required_role: SpeakerRole | null;
  // Auto-end settings
  auto_end_enabled: boolean;
  auto_end_silence_ms: number;
  // Debug storage (development only)
  debug_storage_enabled: boolean;
  // MIIS (Medical Illustration Image Server)
  miis_enabled: boolean;
  miis_server_url: string;
  // Screen capture
  screen_capture_enabled: boolean;
  screen_capture_interval_secs: number;
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
  | 'start_recording'       // Optimistic recording start (before greeting check completes)
  | 'greeting_confirmed'    // Greeting check passed, recording should continue
  | 'greeting_rejected'     // Not a greeting, recording should be discarded
  | 'greeting_detected'     // Legacy: greeting detected
  | 'not_greeting'          // Legacy: not a greeting
  | 'speaker_not_verified'  // Speaker not enrolled or wrong role (if require_enrolled is true)
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

/** Simplified SOAP note - contains raw LLM output as single text block */
export interface SoapNote {
  /** The full SOAP note content as generated by the LLM */
  content: string;
  generated_at: string;
  model_used: string;
}

/** Per-patient SOAP note with speaker identification */
export interface PatientSoapNote {
  /** Label for this patient (e.g., "Patient 1", "Patient 2") */
  patient_label: string;
  /** Which speaker this patient was identified as (e.g., "Speaker 1", "Speaker 3") */
  speaker_id: string;
  /** The SOAP note content for this patient */
  content: string;
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
  /** Custom instructions from the physician (persisted in settings) */
  custom_instructions: string;
  /** Session-specific notes from the clinician (entered during recording) */
  session_notes?: string;
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
// Biomarker Status Helper Functions
// These functions interpret raw biomarker values into clinically meaningful labels
// ============================================================================

export type BiomarkerLevel = 'good' | 'moderate' | 'low';
export type ResponseTimeLevel = 'good' | 'moderate' | 'slow';

export interface BiomarkerStatus {
  label: string;
  level: BiomarkerLevel;
}

export interface ResponseTimeStatus {
  label: string;
  level: ResponseTimeLevel;
}

/**
 * Interpret vitality (pitch variability) value
 * Vitality measures emotional expression through F0 std dev in Hz
 * - High vitality (≥30 Hz): Normal emotional expression
 * - Moderate (15-30 Hz): Reduced expression, worth noting
 * - Low (<15 Hz): Flat affect, clinically notable
 */
export function getVitalityStatus(value: number): BiomarkerStatus {
  if (value >= BIOMARKER_THRESHOLDS.VITALITY_GOOD) {
    return { label: 'Normal', level: 'good' };
  }
  if (value >= BIOMARKER_THRESHOLDS.VITALITY_WARNING) {
    return { label: 'Reduced', level: 'moderate' };
  }
  return { label: 'Low', level: 'low' };
}

/**
 * Interpret stability (CPP - Cepstral Peak Prominence) value
 * Stability measures vocal fold regularity in dB
 * - Good stability (≥8 dB): Regular vocal fold vibration
 * - Moderate (5-8 dB): Some irregularity
 * - Unstable (<5 dB): Tremor, strain, or vocal pathology
 */
export function getStabilityStatus(value: number): BiomarkerStatus {
  if (value >= BIOMARKER_THRESHOLDS.STABILITY_GOOD) {
    return { label: 'Good', level: 'good' };
  }
  if (value >= BIOMARKER_THRESHOLDS.STABILITY_WARNING) {
    return { label: 'Moderate', level: 'moderate' };
  }
  return { label: 'Unstable', level: 'low' };
}

/**
 * Interpret engagement score (0-100)
 * Engagement measures conversation balance and responsiveness
 * - Good engagement (≥70): Active participation
 * - Moderate (40-70): Some interaction issues
 * - Low (<40): Poor conversation dynamics
 */
export function getEngagementStatus(value: number): BiomarkerStatus {
  if (value >= BIOMARKER_THRESHOLDS.ENGAGEMENT_GOOD) {
    return { label: 'Good', level: 'good' };
  }
  if (value >= BIOMARKER_THRESHOLDS.ENGAGEMENT_WARNING) {
    return { label: 'Moderate', level: 'moderate' };
  }
  return { label: 'Low', level: 'low' };
}

/**
 * Interpret response latency (turn-taking speed) in ms
 * Response time measures conversation flow
 * - Quick (≤500ms): Responsive conversation
 * - Moderate (500-1500ms): Normal latency
 * - Slow (>1500ms): Delayed responses, possible cognitive load
 */
export function getResponseTimeStatus(value: number): ResponseTimeStatus {
  if (value <= BIOMARKER_THRESHOLDS.RESPONSE_LATENCY_GOOD) {
    return { label: 'Quick', level: 'good' };
  }
  if (value <= BIOMARKER_THRESHOLDS.RESPONSE_LATENCY_WARNING) {
    return { label: 'Moderate', level: 'moderate' };
  }
  return { label: 'Slow', level: 'slow' };
}

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

// ============================================================================
// Speaker Profile Types (for enrollment-based speaker recognition)
// ============================================================================

/** Speaker role in clinical encounters */
export type SpeakerRole = 'physician' | 'pa' | 'rn' | 'ma' | 'patient' | 'other';

/** Human-readable labels for speaker roles */
export const SPEAKER_ROLE_LABELS: Record<SpeakerRole, string> = {
  physician: 'Physician',
  pa: 'Physician Assistant',
  rn: 'Registered Nurse',
  ma: 'Medical Assistant',
  patient: 'Patient',
  other: 'Other',
};

/** Speaker profile info (without embedding, safe for frontend) */
export interface SpeakerProfileInfo {
  /** Unique identifier (UUID) */
  id: string;
  /** Display name (e.g., "Dr. Smith") */
  name: string;
  /** Role in clinical encounters */
  role: SpeakerRole;
  /** Custom description (e.g., "Internal medicine") */
  description: string;
  /** Creation timestamp (Unix epoch seconds) */
  created_at: number;
  /** Last update timestamp (Unix epoch seconds) */
  updated_at: number;
}

/** Information about a speaker for SOAP generation context */
export interface SpeakerInfo {
  /** Speaker ID as it appears in transcript (e.g., "Dr. Smith", "Speaker 2") */
  id: string;
  /** Description for LLM context (e.g., "Attending physician, internal medicine") */
  description: string;
  /** Whether this speaker was enrolled (recognized) vs auto-detected */
  is_enrolled: boolean;
}

/** Speaker context for SOAP generation */
export interface SpeakerContext {
  /** List of identified speakers with their descriptions */
  speakers: SpeakerInfo[];
}

// ============================================================================
// Local Archive Types (for offline session history)
// ============================================================================

/** Summary of an archived session (for list views) */
export interface LocalArchiveSummary {
  session_id: string;
  date: string;
  duration_ms: number | null;
  word_count: number;
  has_soap_note: boolean;
  has_audio: boolean;
  auto_ended: boolean;
}

/** Metadata for an archived session */
export interface LocalArchiveMetadata {
  session_id: string;
  started_at: string;
  ended_at: string | null;
  duration_ms: number | null;
  segment_count: number;
  word_count: number;
  has_soap_note: boolean;
  has_audio: boolean;
  auto_ended: boolean;
  auto_end_reason: string | null;
  /** SOAP detail level used when generating (1-10), null if no SOAP or pre-feature */
  soap_detail_level: number | null;
  /** SOAP format used when generating ('problem_based' or 'comprehensive'), null if no SOAP or pre-feature */
  soap_format: SoapFormat | null;
}

/** Detailed archived session (for detail view) */
export interface LocalArchiveDetails {
  session_id: string;
  metadata: LocalArchiveMetadata;
  transcript: string | null;
  soap_note: string | null;
  audio_path: string | null;
}

/** Auto-end event payload */
export interface AutoEndEventPayload {
  reason: 'silence';
  silence_duration_ms: number;
}

/** Silence warning event payload (for countdown display) */
export interface SilenceWarningPayload {
  /** Milliseconds of silence so far */
  silence_ms: number;
  /** Milliseconds remaining until auto-end (0 = cancelled/speech detected) */
  remaining_ms: number;
}
