import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emitTo } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

interface SuggestedSplit {
  lineIndex: number;
  confidence: number;
  reason: string;
}

interface SplitContext {
  sessionId: string;
  date: string;
}

/** Parse context from URL query params (set by HistoryWindow when opening) */
function getContextFromUrl(): SplitContext | null {
  const params = new URLSearchParams(window.location.search);
  const sessionId = params.get('sessionId');
  const date = params.get('date');
  if (sessionId && date) {
    return { sessionId, date };
  }
  return null;
}

const SplitWindow: React.FC = () => {
  const [context] = useState<SplitContext | null>(() => getContextFromUrl());
  const [lines, setLines] = useState<string[]>([]);
  const [splitLine, setSplitLine] = useState<number | null>(null);
  const [suggestions, setSuggestions] = useState<SuggestedSplit[]>([]);
  const [loadingSuggestions, setLoadingSuggestions] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [splitting, setSplitting] = useState(false);

  const transcriptRef = useRef<HTMLDivElement>(null);
  const lineRefs = useRef<Map<number, HTMLDivElement>>(new Map());

  // Load transcript lines on mount
  useEffect(() => {
    if (!context) return;

    const loadLines = async () => {
      try {
        const result = await invoke<string[]>('get_session_transcript_lines', {
          sessionId: context.sessionId,
          date: context.date,
        });
        setLines(result);
        if (result.length > 1) {
          setSplitLine(Math.floor(result.length / 2));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    };
    loadLines();
  }, [context]);

  // Fetch LLM suggestions (async, non-blocking)
  useEffect(() => {
    if (!context || lines.length < 2) return;

    setLoadingSuggestions(true);
    invoke<SuggestedSplit[]>('suggest_split_points', {
      sessionId: context.sessionId,
      date: context.date,
    })
      .then((result) => {
        setSuggestions(result);
        // Auto-select the first suggestion if available
        if (result.length > 0) {
          setSplitLine(result[0].lineIndex);
        }
      })
      .catch((e) => {
        console.warn('Failed to get split suggestions:', e);
        // Non-fatal — user can still pick manually
      })
      .finally(() => {
        setLoadingSuggestions(false);
      });
  }, [context, lines.length]);

  // Scroll to a line
  const scrollToLine = useCallback((lineIndex: number) => {
    const el = lineRefs.current.get(lineIndex);
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
  }, []);

  // Handle suggestion click
  const handleSuggestionClick = useCallback((suggestion: SuggestedSplit) => {
    setSplitLine(suggestion.lineIndex);
    scrollToLine(suggestion.lineIndex);
  }, [scrollToLine]);

  // Handle split confirm
  const handleConfirm = useCallback(async () => {
    if (!context || splitLine === null) return;
    setSplitting(true);
    try {
      await invoke<string>('split_local_session', {
        sessionId: context.sessionId,
        date: context.date,
        splitLine,
      });
      // Notify HistoryWindow to refresh
      await emitTo('history', 'split_complete', {});
      const win = getCurrentWindow();
      await win.close();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSplitting(false);
    }
  }, [context, splitLine]);

  // Handle cancel
  const handleCancel = useCallback(async () => {
    const win = getCurrentWindow();
    await win.close();
  }, []);

  // Word counts (memoized — can be expensive for large transcripts)
  const { firstHalfWords, secondHalfWords } = useMemo(() => {
    if (splitLine === null) return { firstHalfWords: 0, secondHalfWords: 0 };
    return {
      firstHalfWords: lines.slice(0, splitLine).join(' ').split(/\s+/).filter(Boolean).length,
      secondHalfWords: lines.slice(splitLine).join(' ').split(/\s+/).filter(Boolean).length,
    };
  }, [lines, splitLine]);

  // Confidence badge color
  const confidenceColor = (confidence: number): string => {
    if (confidence >= 0.8) return '#22c55e';
    if (confidence >= 0.5) return '#eab308';
    return '#ef4444';
  };

  // Check if a line is an LLM-suggested split point
  const getSuggestionForLine = (lineIndex: number): SuggestedSplit | undefined => {
    return suggestions.find((s) => s.lineIndex === lineIndex);
  };

  // No context from URL
  if (!context) {
    return (
      <div className="split-window">
        <div className="split-window-error">
          Missing session context. Please open this window from the History view.
        </div>
      </div>
    );
  }

  // Loading transcript
  if (loading) {
    return (
      <div className="split-window">
        <div className="split-window-header">
          <h1>Split Session</h1>
        </div>
        <div className="split-window-loading">
          <div className="spinner-small" /> Loading transcript...
        </div>
      </div>
    );
  }

  // Error or too few lines
  if (error || lines.length < 2) {
    return (
      <div className="split-window">
        <div className="split-window-header">
          <h1>Split Session</h1>
        </div>
        <div className="split-window-error">
          {error || 'Transcript must have at least 2 lines to split.'}
        </div>
        <div className="split-window-footer">
          <button className="cleanup-dialog-btn cancel" onClick={handleCancel}>Close</button>
        </div>
      </div>
    );
  }

  return (
    <div className="split-window">
      <div className="split-window-header">
        <h1>Split Session</h1>
        <p className="split-window-subtitle">
          Click between lines to set the split point. LLM-suggested transitions are highlighted.
        </p>
      </div>

      {/* Stats bar */}
      <div className="split-window-stats">
        <span className="split-stat part-a">Part A: {firstHalfWords.toLocaleString()} words ({splitLine} lines)</span>
        <span className="split-stat part-b">Part B: {secondHalfWords.toLocaleString()} words ({lines.length - (splitLine ?? 0)} lines)</span>
      </div>

      {/* Suggestions panel */}
      {(loadingSuggestions || suggestions.length > 0) && (
        <div className="split-suggestions-panel">
          <div className="split-suggestions-header">
            {loadingSuggestions ? (
              <><div className="spinner-small" /> Analyzing transcript for encounter transitions...</>
            ) : (
              <span>{suggestions.length} suggested split point{suggestions.length !== 1 ? 's' : ''}</span>
            )}
          </div>
          {suggestions.map((s, i) => (
            <button
              key={i}
              className={`split-suggestion ${splitLine === s.lineIndex ? 'active' : ''}`}
              onClick={() => handleSuggestionClick(s)}
            >
              <span className="split-suggestion-badge" style={{ background: confidenceColor(s.confidence) }}>
                Line {s.lineIndex}
              </span>
              <span className="split-suggestion-reason">{s.reason}</span>
              <span className="split-suggestion-confidence">{Math.round(s.confidence * 100)}%</span>
            </button>
          ))}
        </div>
      )}

      {/* Transcript */}
      <div className="split-transcript-container" ref={transcriptRef}>
        {lines.map((line, i) => {
          const suggestion = getSuggestionForLine(i + 1);
          return (
            <React.Fragment key={i}>
              <div
                className={`split-line ${splitLine !== null && i < splitLine ? 'part-a' : 'part-b'}`}
                ref={(el) => {
                  if (el) lineRefs.current.set(i + 1, el);
                }}
              >
                <span className="split-line-number">{i + 1}</span>
                <span className="split-line-text">{line || '\u00A0'}</span>
                {suggestion && (
                  <span
                    className="split-line-suggestion-marker"
                    style={{ borderColor: confidenceColor(suggestion.confidence) }}
                    title={suggestion.reason}
                  />
                )}
              </div>
              {i < lines.length - 1 && (
                <button
                  className={`split-gutter ${splitLine === i + 1 ? 'active' : ''} ${getSuggestionForLine(i + 1) ? 'suggested' : ''}`}
                  onClick={() => setSplitLine(i + 1)}
                  aria-label={`Split after line ${i + 1}`}
                />
              )}
            </React.Fragment>
          );
        })}
      </div>

      {/* Footer */}
      <div className="split-window-footer">
        <button className="cleanup-dialog-btn cancel" onClick={handleCancel}>Cancel</button>
        <button
          className="cleanup-dialog-btn confirm"
          onClick={handleConfirm}
          disabled={splitLine === null || splitting}
        >
          {splitting ? 'Splitting...' : 'Split Here'}
        </button>
      </div>
    </div>
  );
};

export default SplitWindow;
