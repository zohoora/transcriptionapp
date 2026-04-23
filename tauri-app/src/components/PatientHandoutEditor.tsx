import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { formatErrorMessage } from '../utils';

// Parse URL params once at module level (window.location doesn't change)
const params = new URLSearchParams(window.location.search);
const SESSION_ID = params.get('sessionId') || '';
const DATE = params.get('date') || '';

export function PatientHandoutEditor() {
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load handout from archive on mount (saved by the hook before opening this window)
  useEffect(() => {
    let mounted = true;
    invoke<string | null>('get_patient_handout', { sessionId: SESSION_ID, date: DATE })
      .then(result => {
        if (mounted && result) setContent(result);
      })
      .catch(e => {
        if (mounted) setError(formatErrorMessage(e));
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });
    return () => { mounted = false; };
  }, []);

  const handleSave = useCallback(async () => {
    setError(null);
    try {
      await invoke('save_patient_handout', { sessionId: SESSION_ID, date: DATE, content });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(formatErrorMessage(e));
    }
  }, [content]);

  const handleCopy = useCallback(async () => {
    try {
      await writeText(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      setError(formatErrorMessage(e));
    }
  }, [content]);

  const handlePrint = useCallback(() => { window.print(); }, []);
  // `window.close()` is a no-op for Tauri webviews that weren't opened via
  // `window.open()` — this editor is launched by WebviewWindow so we have
  // to ask Tauri to close the current window directly.
  const handleClose = useCallback(() => {
    getCurrentWebviewWindow().close().catch((e) => {
      console.error('Failed to close patient handout window:', e);
    });
  }, []);

  return (
    <>
      <style>{`
        .handout-editor {
          display: flex; flex-direction: column; height: 100vh;
          background: #fff; color: #1a1a1a;
          font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        }
        .handout-header {
          padding: 12px 16px; border-bottom: 1px solid #e5e7eb;
          display: flex; align-items: center; justify-content: space-between; flex-shrink: 0;
        }
        .handout-title { font-size: 16px; font-weight: 600; color: #111827; }
        .handout-body {
          flex: 1; padding: 16px; overflow: hidden;
          display: flex; flex-direction: column;
        }
        .handout-textarea {
          flex: 1; width: 100%; border: 1px solid #d1d5db; border-radius: 6px;
          padding: 16px; font-family: Georgia, 'Times New Roman', serif;
          font-size: 14px; line-height: 1.7; color: #1a1a1a; resize: none; outline: none;
        }
        .handout-textarea:focus { border-color: #3b82f6; box-shadow: 0 0 0 2px rgba(59,130,246,0.15); }
        .handout-error {
          margin-top: 8px; padding: 8px 12px; background: #fef2f2;
          color: #991b1b; border-radius: 4px; font-size: 13px;
        }
        .handout-toolbar {
          display: flex; gap: 8px; padding: 12px 16px;
          border-top: 1px solid #e5e7eb; flex-shrink: 0;
        }
        .handout-btn {
          padding: 8px 16px; border-radius: 6px; font-size: 13px; font-weight: 500;
          cursor: pointer; border: 1px solid #d1d5db; background: #fff; color: #374151;
          transition: background 0.15s, border-color 0.15s;
        }
        .handout-btn:hover { background: #f9fafb; border-color: #9ca3af; }
        .handout-btn-primary { background: #2563eb; color: #fff; border-color: #2563eb; }
        .handout-btn-primary:hover { background: #1d4ed8; border-color: #1d4ed8; }
        .handout-btn-success { background: #059669; color: #fff; border-color: #059669; }
        .handout-toolbar-spacer { flex: 1; }
        @media print {
          .handout-header, .handout-toolbar, .handout-error { display: none !important; }
          .handout-editor { height: auto; }
          .handout-body { padding: 0; overflow: visible; }
          .handout-textarea {
            border: none; padding: 0; font-family: Georgia, 'Times New Roman', serif;
            font-size: 12pt; line-height: 1.6; white-space: pre-wrap;
            overflow: visible; height: auto;
          }
          @page { margin: 1in; }
        }
      `}</style>
      <div className="handout-editor">
        <div className="handout-header">
          <span className="handout-title">Patient Handout</span>
        </div>
        <div className="handout-body">
          {loading ? (
            <div style={{ padding: 16, color: '#6b7280' }}>Loading handout...</div>
          ) : (
            <textarea
              className="handout-textarea"
              value={content}
              onChange={(e) => setContent(e.target.value)}
              placeholder="Handout content will appear here..."
            />
          )}
          {error && <div className="handout-error">{error}</div>}
        </div>
        <div className="handout-toolbar">
          <button
            className={`handout-btn ${saved ? 'handout-btn-success' : 'handout-btn-primary'}`}
            onClick={handleSave}
            disabled={!content.trim() || loading}
          >
            {saved ? 'Saved!' : 'Save'}
          </button>
          <button className="handout-btn" onClick={handlePrint} disabled={!content.trim() || loading}>
            Print
          </button>
          <button
            className={`handout-btn ${copied ? 'handout-btn-success' : ''}`}
            onClick={handleCopy}
            disabled={!content.trim() || loading}
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
          <div className="handout-toolbar-spacer" />
          <button className="handout-btn" onClick={handleClose}>Close</button>
        </div>
      </div>
    </>
  );
}
