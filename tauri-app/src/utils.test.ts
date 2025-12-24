import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  formatTime,
  truncateText,
  countWords,
  splitIntoParagraphs,
  debounce,
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
