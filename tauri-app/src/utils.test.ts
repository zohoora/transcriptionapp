import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  formatTime,
  truncateText,
  countWords,
  splitIntoParagraphs,
  debounce,
  formatDateForApi,
  formatLocalDateTime,
  formatLocalTime,
  formatLocalDate,
  isSameLocalDay,
  isToday,
  formatDuration,
  formatErrorMessage,
} from './utils';

describe('formatTime', () => {
  it('formats 0 milliseconds correctly', () => {
    expect(formatTime(0)).toBe('00:00');
  });

  it('formats seconds under a minute', () => {
    expect(formatTime(5000)).toBe('00:05');
    expect(formatTime(30000)).toBe('00:30');
    expect(formatTime(59000)).toBe('00:59');
  });

  it('formats minutes under an hour', () => {
    expect(formatTime(60000)).toBe('01:00');
    expect(formatTime(65000)).toBe('01:05');
    expect(formatTime(600000)).toBe('10:00');
    expect(formatTime(3540000)).toBe('59:00');
  });

  it('formats times over an hour with HH:MM:SS', () => {
    expect(formatTime(3600000)).toBe('01:00:00');
    expect(formatTime(3665000)).toBe('01:01:05');
    expect(formatTime(7200000)).toBe('02:00:00');
    expect(formatTime(36000000)).toBe('10:00:00');
  });

  it('pads single digit values with zeros', () => {
    expect(formatTime(1000)).toBe('00:01');
    expect(formatTime(61000)).toBe('01:01');
    expect(formatTime(3661000)).toBe('01:01:01');
  });

  it('handles large durations', () => {
    // 25 hours
    expect(formatTime(90000000)).toBe('25:00:00');
  });

  it('truncates partial seconds', () => {
    expect(formatTime(1500)).toBe('00:01');
    expect(formatTime(1999)).toBe('00:01');
    expect(formatTime(2000)).toBe('00:02');
  });
});

describe('truncateText', () => {
  it('returns text unchanged if under max length', () => {
    expect(truncateText('Hello', 10)).toBe('Hello');
    expect(truncateText('Hello', 5)).toBe('Hello');
  });

  it('truncates text and adds ellipsis if over max length', () => {
    expect(truncateText('Hello World', 8)).toBe('Hello...');
    expect(truncateText('Hello World', 10)).toBe('Hello W...');
  });

  it('handles edge cases', () => {
    expect(truncateText('', 10)).toBe('');
    expect(truncateText('Hi', 3)).toBe('Hi');
    expect(truncateText('Hello', 4)).toBe('H...');
  });

  it('handles exact length match', () => {
    expect(truncateText('Hello', 5)).toBe('Hello');
  });
});

describe('countWords', () => {
  it('counts words correctly', () => {
    expect(countWords('Hello world')).toBe(2);
    expect(countWords('One two three four')).toBe(4);
    expect(countWords('Single')).toBe(1);
  });

  it('handles empty strings', () => {
    expect(countWords('')).toBe(0);
    expect(countWords('   ')).toBe(0);
  });

  it('handles multiple spaces between words', () => {
    expect(countWords('Hello    world')).toBe(2);
    expect(countWords('  One   two   three  ')).toBe(3);
  });

  it('handles newlines and tabs', () => {
    expect(countWords('Hello\nworld')).toBe(2);
    expect(countWords('Hello\tworld')).toBe(2);
    expect(countWords('Hello\n\nworld')).toBe(2);
  });

  it('handles punctuation attached to words', () => {
    expect(countWords('Hello, world!')).toBe(2);
    expect(countWords("It's a test.")).toBe(3);
  });
});

describe('splitIntoParagraphs', () => {
  it('splits text on double newlines', () => {
    const text = 'First paragraph.\n\nSecond paragraph.';
    expect(splitIntoParagraphs(text)).toEqual([
      'First paragraph.',
      'Second paragraph.',
    ]);
  });

  it('handles single paragraph', () => {
    expect(splitIntoParagraphs('Just one paragraph')).toEqual([
      'Just one paragraph',
    ]);
  });

  it('handles empty string', () => {
    expect(splitIntoParagraphs('')).toEqual([]);
  });

  it('filters out empty paragraphs', () => {
    const text = 'First.\n\n\n\nSecond.\n\n';
    const result = splitIntoParagraphs(text);
    expect(result).toEqual(['First.', 'Second.']);
  });

  it('preserves single newlines within paragraphs', () => {
    const text = 'Line one\nLine two\n\nParagraph two';
    expect(splitIntoParagraphs(text)).toEqual([
      'Line one\nLine two',
      'Paragraph two',
    ]);
  });

  it('handles multiple paragraphs', () => {
    const text = 'One\n\nTwo\n\nThree\n\nFour';
    expect(splitIntoParagraphs(text)).toHaveLength(4);
  });
});

describe('debounce', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('delays function execution', () => {
    const fn = vi.fn();
    const debouncedFn = debounce(fn, 100);

    debouncedFn();
    expect(fn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(50);
    expect(fn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(50);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('cancels previous calls when called again', () => {
    const fn = vi.fn();
    const debouncedFn = debounce(fn, 100);

    debouncedFn();
    vi.advanceTimersByTime(50);

    debouncedFn(); // Should reset the timer
    vi.advanceTimersByTime(50);
    expect(fn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(50);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('passes arguments to the debounced function', () => {
    const fn = vi.fn();
    const debouncedFn = debounce(fn, 100);

    debouncedFn('arg1', 'arg2');
    vi.advanceTimersByTime(100);

    expect(fn).toHaveBeenCalledWith('arg1', 'arg2');
  });

  it('uses the latest arguments when called multiple times', () => {
    const fn = vi.fn();
    const debouncedFn = debounce(fn, 100);

    debouncedFn('first');
    debouncedFn('second');
    debouncedFn('third');

    vi.advanceTimersByTime(100);

    expect(fn).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenCalledWith('third');
  });

  it('allows multiple completed calls over time', () => {
    const fn = vi.fn();
    const debouncedFn = debounce(fn, 100);

    debouncedFn();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);

    debouncedFn();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(2);
  });
});

// ============================================================================
// Date/Time Utilities Tests
// ============================================================================

describe('formatDateForApi', () => {
  it('formats date as YYYY-MM-DD in UTC', () => {
    // Create a date that's Jan 15, 2024 at midnight UTC
    const date = new Date(Date.UTC(2024, 0, 15, 0, 0, 0));
    expect(formatDateForApi(date)).toBe('2024-01-15');
  });

  it('uses UTC date, not local date', () => {
    // Create a date that's Jan 15, 2024 at 23:00 UTC
    // In UTC-8, this would be Jan 15 3:00 PM local
    // In UTC+12, this would be Jan 16 11:00 AM local
    // But the API date should always be the UTC date
    const date = new Date(Date.UTC(2024, 0, 15, 23, 0, 0));
    expect(formatDateForApi(date)).toBe('2024-01-15');
  });

  it('pads single digit months and days', () => {
    const date = new Date(Date.UTC(2024, 0, 5, 0, 0, 0)); // Jan 5
    expect(formatDateForApi(date)).toBe('2024-01-05');
  });

  it('handles December correctly', () => {
    const date = new Date(Date.UTC(2024, 11, 31, 0, 0, 0)); // Dec 31
    expect(formatDateForApi(date)).toBe('2024-12-31');
  });
});

describe('formatLocalDateTime', () => {
  it('returns a locale-formatted date and time string', () => {
    const result = formatLocalDateTime('2024-01-15T14:30:00Z');
    // The exact format depends on locale, but it should contain date and time parts
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });

  it('handles ISO strings with timezone offset', () => {
    const result = formatLocalDateTime('2024-01-15T14:30:00+05:00');
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });
});

describe('formatLocalTime', () => {
  it('returns a locale-formatted time string', () => {
    const result = formatLocalTime('2024-01-15T14:30:00Z');
    // Should contain hour and minute
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });

  it('formats different times correctly', () => {
    // These should produce different outputs
    const morning = formatLocalTime('2024-01-15T08:00:00Z');
    const evening = formatLocalTime('2024-01-15T20:00:00Z');
    // In most timezones, these will be different
    // (unless timezone offset is exactly 12 hours)
    expect(morning).not.toBe(evening);
  });
});

describe('formatLocalDate', () => {
  it('returns a locale-formatted date string', () => {
    const result = formatLocalDate('2024-01-15T14:30:00Z');
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });

  it('handles different dates', () => {
    const jan = formatLocalDate('2024-01-15T12:00:00Z');
    const dec = formatLocalDate('2024-12-25T12:00:00Z');
    expect(jan).not.toBe(dec);
  });
});

describe('isSameLocalDay', () => {
  it('returns true for same day', () => {
    const date1 = new Date(2024, 0, 15, 10, 0, 0); // Jan 15, 10:00 AM
    const date2 = new Date(2024, 0, 15, 22, 0, 0); // Jan 15, 10:00 PM
    expect(isSameLocalDay(date1, date2)).toBe(true);
  });

  it('returns false for different days', () => {
    const date1 = new Date(2024, 0, 15, 10, 0, 0); // Jan 15
    const date2 = new Date(2024, 0, 16, 10, 0, 0); // Jan 16
    expect(isSameLocalDay(date1, date2)).toBe(false);
  });

  it('returns false for different months', () => {
    const date1 = new Date(2024, 0, 15); // Jan 15
    const date2 = new Date(2024, 1, 15); // Feb 15
    expect(isSameLocalDay(date1, date2)).toBe(false);
  });

  it('returns false for different years', () => {
    const date1 = new Date(2024, 0, 15);
    const date2 = new Date(2025, 0, 15);
    expect(isSameLocalDay(date1, date2)).toBe(false);
  });

  it('handles midnight boundary', () => {
    const beforeMidnight = new Date(2024, 0, 15, 23, 59, 59);
    const afterMidnight = new Date(2024, 0, 16, 0, 0, 1);
    expect(isSameLocalDay(beforeMidnight, afterMidnight)).toBe(false);
  });
});

describe('isToday', () => {
  it('returns true for today', () => {
    const today = new Date();
    expect(isToday(today)).toBe(true);
  });

  it('returns true for today at different times', () => {
    const now = new Date();
    const todayMorning = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 8, 0, 0);
    const todayEvening = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 20, 0, 0);
    expect(isToday(todayMorning)).toBe(true);
    expect(isToday(todayEvening)).toBe(true);
  });

  it('returns false for yesterday', () => {
    const now = new Date();
    const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
    expect(isToday(yesterday)).toBe(false);
  });

  it('returns false for tomorrow', () => {
    const now = new Date();
    const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
    expect(isToday(tomorrow)).toBe(false);
  });

  it('returns false for same day last year', () => {
    const now = new Date();
    const lastYear = new Date(now.getFullYear() - 1, now.getMonth(), now.getDate());
    expect(isToday(lastYear)).toBe(false);
  });
});

describe('formatDuration', () => {
  it('formats minutes only for short durations', () => {
    expect(formatDuration(0)).toBe('0m');
    expect(formatDuration(60000)).toBe('1m');
    expect(formatDuration(300000)).toBe('5m');
    expect(formatDuration(3540000)).toBe('59m');
  });

  it('formats hours and minutes for longer durations', () => {
    expect(formatDuration(3600000)).toBe('1h 0m');
    expect(formatDuration(3660000)).toBe('1h 1m');
    expect(formatDuration(7200000)).toBe('2h 0m');
    expect(formatDuration(5400000)).toBe('1h 30m');
  });

  it('handles multi-hour durations', () => {
    expect(formatDuration(10800000)).toBe('3h 0m');
    expect(formatDuration(12600000)).toBe('3h 30m');
  });

  it('truncates partial minutes', () => {
    expect(formatDuration(90000)).toBe('1m'); // 1.5 minutes
    expect(formatDuration(150000)).toBe('2m'); // 2.5 minutes
  });
});

// ============================================================================
// Error Utilities Tests
// ============================================================================

describe('formatErrorMessage', () => {
  it('extracts message from Error objects', () => {
    const error = new Error('Something went wrong');
    expect(formatErrorMessage(error)).toBe('Something went wrong');
  });

  it('handles Error objects with empty message', () => {
    const error = new Error('');
    expect(formatErrorMessage(error)).toBe('Error'); // Falls back to name
  });

  it('strips "Error: " prefix from strings', () => {
    expect(formatErrorMessage('Error: Connection failed')).toBe('Connection failed');
  });

  it('returns string errors unchanged if no prefix', () => {
    expect(formatErrorMessage('Connection failed')).toBe('Connection failed');
  });

  it('handles null', () => {
    expect(formatErrorMessage(null)).toBe('Unknown error');
  });

  it('handles undefined', () => {
    expect(formatErrorMessage(undefined)).toBe('Unknown error');
  });

  it('handles objects with message property', () => {
    const error = { message: 'Network timeout', code: 'TIMEOUT' };
    expect(formatErrorMessage(error)).toBe('Network timeout');
  });

  it('handles empty string', () => {
    expect(formatErrorMessage('')).toBe('Unknown error');
  });

  it('strips prefix from stringified errors', () => {
    // When String(error) returns "Error: message"
    expect(formatErrorMessage('Error: Failed to connect')).toBe('Failed to connect');
  });

  it('handles numbers', () => {
    expect(formatErrorMessage(404)).toBe('404');
  });

  it('handles complex Tauri-style errors', () => {
    // Tauri errors often have this structure
    const tauriError = {
      message: 'Command not found: unknown_command',
      name: 'TauriError',
    };
    expect(formatErrorMessage(tauriError)).toBe('Command not found: unknown_command');
  });
});
