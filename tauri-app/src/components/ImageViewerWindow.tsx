import { useState, useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';

/**
 * Standalone window for displaying AI-generated medical illustrations.
 * Receives image data via a Tauri event after the window opens.
 * The image fills the window and resizes with it.
 */
export default function ImageViewerWindow() {
  const [imageBase64, setImageBase64] = useState<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    let unlisten: (() => void) | undefined;

    listen<{ base64: string }>('image_viewer_data', (event) => {
      if (mounted.current) {
        setImageBase64(event.payload.base64);
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
      <div style={{
        width: '100vw',
        height: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: '#1a1a1a',
        color: '#666',
        fontSize: '14px',
      }}>
        Loading image...
      </div>
    );
  }

  return (
    <div style={{
      width: '100vw',
      height: '100vh',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      backgroundColor: '#1a1a1a',
      overflow: 'hidden',
    }}>
      <img
        src={`data:image/png;base64,${imageBase64}`}
        alt="AI-generated medical illustration"
        style={{
          maxWidth: '100%',
          maxHeight: '100%',
          objectFit: 'contain',
        }}
      />
    </div>
  );
}
