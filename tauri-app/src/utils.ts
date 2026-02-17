/**
 * Format milliseconds to a human-readable time string
 * Returns "MM:SS" for times under an hour, "HH:MM:SS" for longer times
 */
export function formatTime(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  }
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
}

/**
 * Truncate text to a maximum length, adding ellipsis if needed
 */
export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength - 3) + '...';
}

/**
 * Word count for transcript
 */
export function countWords(text: string): number {
  if (!text || !text.trim()) return 0;
  return text.trim().split(/\s+/).length;
}

/**
 * Parse paragraph breaks from transcription
 */
export function splitIntoParagraphs(text: string): string[] {
  if (!text) return [];
  return text.split('\n\n').filter(p => p.trim().length > 0);
}

/**
 * Debounce function for UI updates
 */
export function debounce<T extends (...args: unknown[]) => unknown>(
  fn: T,
  delay: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout>;
  return (...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  };
}

// ============================================================================
// Date/Time Utilities
// ============================================================================
//
// PRINCIPLE: Store UTC, Display Local
// - All dates sent to API should be in UTC
// - All dates displayed to user should be in local timezone

/**
 * Format a Date for API queries (YYYY-MM-DD in UTC)
 * Use this when sending date parameters to the backend
 */
export function formatDateForApi(date: Date): string {
  const year = date.getUTCFullYear();
  const month = String(date.getUTCMonth() + 1).padStart(2, '0');
  const day = String(date.getUTCDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

/**
 * Parse an ISO/RFC3339 string and format for local display (date + time)
 */
export function formatLocalDateTime(isoString: string): string {
  return new Date(isoString).toLocaleString();
}

/**
 * Parse an ISO/RFC3339 string and format just the time for local display
 */
export function formatLocalTime(isoString: string): string {
  return new Date(isoString).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit'
  });
}

/**
 * Parse an ISO/RFC3339 string and format just the date for local display
 */
export function formatLocalDate(isoString: string): string {
  return new Date(isoString).toLocaleDateString();
}

/**
 * Check if two dates are the same calendar day (in local timezone)
 */
export function isSameLocalDay(date1: Date, date2: Date): boolean {
  return date1.getFullYear() === date2.getFullYear() &&
         date1.getMonth() === date2.getMonth() &&
         date1.getDate() === date2.getDate();
}

/**
 * Check if a date is today (in local timezone)
 */
export function isToday(date: Date): boolean {
  return isSameLocalDay(date, new Date());
}

/**
 * Format a duration in milliseconds to a human-readable string
 * Returns "Xh Ym" or "Ym" depending on duration
 */
export function formatDuration(ms: number): string {
  const totalMinutes = Math.floor(ms / 60000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  return `${minutes}m`;
}

// ============================================================================
// Error Utilities
// ============================================================================

// ============================================================================
// Audio Quality Utilities
// ============================================================================

import type { AudioQualitySnapshot, SpeakerBiomarkers } from './types';
import { AUDIO_QUALITY_THRESHOLDS } from './types';

export type AudioQualityLevel = 'good' | 'fair' | 'poor';

/**
 * Evaluate audio quality snapshot and return a level.
 * Used by RecordingMode, ContinuousMode, and ReviewMode.
 */
export function getAudioQualityLevel(quality: AudioQualitySnapshot | null): AudioQualityLevel {
  if (!quality) return 'good';

  const rmsOk = quality.rms_db >= AUDIO_QUALITY_THRESHOLDS.LEVEL_TOO_QUIET
             && quality.rms_db <= AUDIO_QUALITY_THRESHOLDS.LEVEL_TOO_HOT;
  const snrOk = quality.snr_db >= AUDIO_QUALITY_THRESHOLDS.SNR_GOOD;
  const clippingOk = quality.clipped_ratio < AUDIO_QUALITY_THRESHOLDS.CLIPPING_OK;

  if (rmsOk && snrOk && clippingOk) return 'good';
  if (quality.snr_db < AUDIO_QUALITY_THRESHOLDS.SNR_WARNING || quality.clipped_ratio >= 0.01) return 'poor';
  return 'fair';
}

// ============================================================================
// Patient Speaker Aggregation
// ============================================================================

export interface AggregatedPatientMetrics {
  vitality: number | null;
  stability: number | null;
  totalUtterances: number;
}

/**
 * Pool all non-clinician speakers into one aggregate via weighted average
 * by talk_time_ms. If no enrolled clinicians exist, all speakers are treated
 * as patient.
 *
 * Used by PatientPulse (with engagement) and usePatientBiomarkers (for trending).
 */
export function aggregatePatientSpeakers(speakers: SpeakerBiomarkers[]): AggregatedPatientMetrics {
  const hasClinicians = speakers.some(s => s.is_clinician);
  const patients = hasClinicians ? speakers.filter(s => !s.is_clinician) : speakers;

  if (patients.length === 0) {
    return { vitality: null, stability: null, totalUtterances: 0 };
  }

  const totalTalkTime = patients.reduce((sum, s) => sum + s.talk_time_ms, 0);
  const totalUtterances = patients.reduce((sum, s) => sum + s.utterance_count, 0);

  let vitality: number | null = null;
  let stability: number | null = null;

  if (totalTalkTime > 0) {
    // Weighted average for vitality
    const vSpeakers = patients.filter(s => s.vitality_mean !== null);
    if (vSpeakers.length > 0) {
      const vTalkTime = vSpeakers.reduce((sum, s) => sum + s.talk_time_ms, 0);
      if (vTalkTime > 0) {
        vitality = vSpeakers.reduce(
          (sum, s) => sum + (s.vitality_mean! * s.talk_time_ms), 0,
        ) / vTalkTime;
      }
    }

    // Weighted average for stability
    const sSpeakers = patients.filter(s => s.stability_mean !== null);
    if (sSpeakers.length > 0) {
      const sTalkTime = sSpeakers.reduce((sum, s) => sum + s.talk_time_ms, 0);
      if (sTalkTime > 0) {
        stability = sSpeakers.reduce(
          (sum, s) => sum + (s.stability_mean! * s.talk_time_ms), 0,
        ) / sTalkTime;
      }
    }
  }

  return { vitality, stability, totalUtterances };
}

// ============================================================================
// Error Utilities
// ============================================================================

/**
 * Extract a user-friendly error message from various error types.
 * Handles: Error objects, Tauri errors, string errors, and unknown values.
 */
export function formatErrorMessage(error: unknown): string {
  if (error === null || error === undefined) {
    return 'Unknown error';
  }

  // Handle Error objects
  if (error instanceof Error) {
    // Strip "Error: " prefix if present
    return error.message || error.name || 'Unknown error';
  }

  // Handle string errors
  if (typeof error === 'string') {
    // Strip common prefixes
    if (error.startsWith('Error: ')) {
      return error.slice(7);
    }
    return error || 'Unknown error';
  }

  // Handle objects with message property (like Tauri errors)
  if (typeof error === 'object' && 'message' in error) {
    const msg = (error as { message: unknown }).message;
    if (typeof msg === 'string') {
      return msg;
    }
  }

  // Fallback to string conversion
  try {
    const str = String(error);
    if (str.startsWith('Error: ')) {
      return str.slice(7);
    }
    return str || 'Unknown error';
  } catch {
    return 'Unknown error';
  }
}
