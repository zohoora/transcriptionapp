import { useState, useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { downloadBase64Image } from '../utils';

/**
 * Standalone window for displaying AI-generated medical illustrations.
 * Receives image data via a Tauri event after the window opens.
 * Toolbar: Save to disk, Print, Close.
 */
export default function ImageViewerWindow() {
  const [imageBase64, setImageBase64] = useState<string | null>(null);
  const [prompt, setPrompt] = useState<string>('');
  const [saved, setSaved] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    let unlisten: (() => void) | undefined;

    listen<{ base64: string; prompt?: string }>('image_viewer_data', (event) => {
      if (mounted.current) {
        setImageBase64(event.payload.base64);
        setPrompt(event.payload.prompt || '');
        setSaved(false);
      }
    }).then((fn) => {
      if (mounted.current) {
        unlisten = fn;
      } else {
        fn();
      }
    });

    return () => {
      mounted.current = false;
      unlisten?.();
    };
  }, []);

  const handleSave = useCallback(() => {
    if (!imageBase64) return;
    downloadBase64Image(imageBase64, 'illustration');
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [imageBase64]);

  const handlePrint = useCallback(() => {
    window.print();
  }, []);

  const handleClose = useCallback(async () => {
    window.close();
  }, []);

  if (!imageBase64) {
    return (
      <div className="image-viewer-container">
        <div className="image-viewer-loading">Loading image...</div>
      </div>
    );
  }

  return (
    <div className="image-viewer-container">
      <div className="image-viewer-toolbar">
        {prompt && <span className="image-viewer-prompt">{prompt}</span>}
        <div className="image-viewer-actions">
          <button className="image-viewer-btn" onClick={handleSave}>
            {saved ? 'Saved' : 'Save'}
          </button>
          <button className="image-viewer-btn" onClick={handlePrint}>Print</button>
          <button className="image-viewer-btn image-viewer-btn-close" onClick={handleClose}>Close</button>
        </div>
      </div>
      <div className="image-viewer-body">
        <img
          src={`data:image/png;base64,${imageBase64}`}
          alt="AI-generated medical illustration"
          className="image-viewer-img"
        />
      </div>
    </div>
  );
}
