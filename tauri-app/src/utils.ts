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
