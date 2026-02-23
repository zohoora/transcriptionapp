import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface SplitViewProps {
  sessionId: string;
  date: string;
  onConfirm: (splitLine: number) => void;
  onCancel: () => void;
}

const SplitView: React.FC<SplitViewProps> = ({
  sessionId,
  date,
  onConfirm,
  onCancel,
}) => {
  const [lines, setLines] = useState<string[]>([]);
  const [splitLine, setSplitLine] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const loadLines = async () => {
      try {
        const result = await invoke<string[]>('get_session_transcript_lines', {
          sessionId,
          date,
        });
        setLines(result);
        // Default split at middle
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
  }, [sessionId, date]);

  const firstHalfWords = splitLine !== null
    ? lines.slice(0, splitLine).join(' ').split(/\s+/).filter(Boolean).length
    : 0;
  const secondHalfWords = splitLine !== null
    ? lines.slice(splitLine).join(' ').split(/\s+/).filter(Boolean).length
    : 0;

  if (loading) {
    return (
      <div className="cleanup-dialog-overlay" onClick={onCancel}>
        <div className="cleanup-dialog split-dialog" onClick={(e) => e.stopPropagation()}>
          <div className="cleanup-dialog-loading"><div className="spinner-small" /> Loading transcript...</div>
        </div>
      </div>
    );
  }

  if (error || lines.length < 2) {
    return (
      <div className="cleanup-dialog-overlay" onClick={onCancel}>
        <div className="cleanup-dialog split-dialog" onClick={(e) => e.stopPropagation()}>
          <h3>Split Session</h3>
          <p className="cleanup-dialog-warning">
            {error || 'Transcript must have at least 2 lines to split.'}
          </p>
          <div className="cleanup-dialog-actions">
            <button className="cleanup-dialog-btn cancel" onClick={onCancel}>Close</button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="cleanup-dialog-overlay" onClick={onCancel}>
      <div className="cleanup-dialog split-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>Split Session</h3>
        <p className="cleanup-dialog-subtitle">
          Click between lines to set the split point. Lines above stay in the original session.
        </p>

        <div className="split-preview-stats">
          <span>Part A: {firstHalfWords} words ({splitLine} lines)</span>
          <span>Part B: {secondHalfWords} words ({lines.length - (splitLine ?? 0)} lines)</span>
        </div>

        <div className="split-transcript">
          {lines.map((line, i) => (
            <React.Fragment key={i}>
              <div
                className={`split-line ${
                  splitLine !== null && i < splitLine ? 'part-a' : 'part-b'
                }`}
              >
                <span className="split-line-number">{i + 1}</span>
                <span className="split-line-text">{line || '\u00A0'}</span>
              </div>
              {i < lines.length - 1 && (
                <button
                  className={`split-gutter ${splitLine === i + 1 ? 'active' : ''}`}
                  onClick={() => setSplitLine(i + 1)}
                  aria-label={`Split after line ${i + 1}`}
                />
              )}
            </React.Fragment>
          ))}
        </div>

        <div className="cleanup-dialog-actions">
          <button className="cleanup-dialog-btn cancel" onClick={onCancel}>Cancel</button>
          <button
            className="cleanup-dialog-btn confirm"
            onClick={() => splitLine !== null && onConfirm(splitLine)}
            disabled={splitLine === null}
          >
            Split Here
          </button>
        </div>
      </div>
    </div>
  );
};

export default SplitView;
