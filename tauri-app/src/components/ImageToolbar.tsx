import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { writeImage } from '@tauri-apps/plugin-clipboard-manager';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { formatErrorMessage } from '../utils';

interface ImageToolbarProps {
  imageBase64: string | null;
  prompt?: string;
  /**
   * Called when the user clicks the rightmost button. Defaults to closing
   * the current webview window (the common "this is a standalone window"
   * case). Override for in-window navigation, e.g. ImageHistoryWindow's
   * detail view → grid view.
   */
  onClose?: () => void;
  closeLabel?: string;
}

const base64ToBytes = (b64: string): Uint8Array =>
  Uint8Array.from(atob(b64), c => c.charCodeAt(0));

/**
 * Toolbar for AI image viewer windows: Save (native dialog) / Print (native
 * NSPrintOperation via osascript Preview) / Copy (image to clipboard) /
 * Close-or-Back. Shared between ImageViewerWindow and ImageHistoryWindow so
 * a fix to any one action lands in both.
 */
export function ImageToolbar({
  imageBase64,
  prompt,
  onClose,
  closeLabel = 'Close',
}: ImageToolbarProps) {
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);
  const [printing, setPrinting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const disabled = !imageBase64;

  // Cancel pending flash timers on rapid re-clicks or unmount so we don't
  // setState after the user already closed the window.
  const savedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const copiedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => {
    if (savedTimer.current) clearTimeout(savedTimer.current);
    if (copiedTimer.current) clearTimeout(copiedTimer.current);
  }, []);

  const handleSave = useCallback(async () => {
    if (!imageBase64) return;
    setError(null);
    try {
      const ts = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      const path = await save({
        title: 'Save illustration',
        defaultPath: `illustration-${ts}.png`,
        filters: [{ name: 'PNG image', extensions: ['png'] }],
      });
      if (!path) return;
      await invoke('save_image_png', { imageBase64, destPath: path });
      setSaved(true);
      if (savedTimer.current) clearTimeout(savedTimer.current);
      savedTimer.current = setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(formatErrorMessage(e));
    }
  }, [imageBase64]);

  const handlePrint = useCallback(async () => {
    if (!imageBase64) return;
    setError(null);
    setPrinting(true);
    try {
      await invoke('print_image_png', { imageBase64 });
    } catch (e) {
      setError(formatErrorMessage(e));
    } finally {
      setPrinting(false);
    }
  }, [imageBase64]);

  const handleCopy = useCallback(async () => {
    if (!imageBase64) return;
    setError(null);
    try {
      await writeImage(base64ToBytes(imageBase64));
      setCopied(true);
      if (copiedTimer.current) clearTimeout(copiedTimer.current);
      copiedTimer.current = setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      setError(formatErrorMessage(e));
    }
  }, [imageBase64]);

  const handleClose = useCallback(() => {
    if (onClose) {
      onClose();
      return;
    }
    getCurrentWebviewWindow().close().catch((e) => {
      console.error('Failed to close window:', e);
    });
  }, [onClose]);

  return (
    <>
      <div className="image-viewer-toolbar">
        {prompt && <span className="image-viewer-prompt">{prompt}</span>}
        <div className="image-viewer-actions">
          <button className="image-viewer-btn" onClick={handleSave} disabled={disabled}>
            {saved ? 'Saved' : 'Save'}
          </button>
          <button className="image-viewer-btn" onClick={handlePrint} disabled={disabled || printing}>
            {printing ? 'Printing…' : 'Print'}
          </button>
          <button className="image-viewer-btn" onClick={handleCopy} disabled={disabled}>
            {copied ? 'Copied' : 'Copy'}
          </button>
          <button className="image-viewer-btn image-viewer-btn-close" onClick={handleClose}>
            {closeLabel}
          </button>
        </div>
      </div>
      {error && <div className="image-viewer-error">{error}</div>}
    </>
  );
}
