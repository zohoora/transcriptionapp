import { useState, useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ImageToolbar } from './ImageToolbar';

/**
 * Standalone window for displaying AI-generated medical illustrations.
 * Receives image data via a Tauri event after the window opens.
 * Toolbar: Save / Print / Copy / Close (shared with ImageHistoryWindow).
 */
export default function ImageViewerWindow() {
  const [imageBase64, setImageBase64] = useState<string | null>(null);
  const [prompt, setPrompt] = useState<string>('');
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    let unlisten: (() => void) | undefined;

    listen<{ base64: string; prompt?: string }>('image_viewer_data', (event) => {
      if (mounted.current) {
        setImageBase64(event.payload.base64);
        setPrompt(event.payload.prompt || '');
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

  if (!imageBase64) {
    return (
      <div className="image-viewer-container">
        <div className="image-viewer-loading">Loading image...</div>
      </div>
    );
  }

  return (
    <div className="image-viewer-container">
      <ImageToolbar imageBase64={imageBase64} prompt={prompt} />
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
