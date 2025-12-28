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
