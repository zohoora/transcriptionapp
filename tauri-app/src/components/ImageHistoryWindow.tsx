import { useState, useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { downloadBase64Image } from '../utils';

interface HistoryImage {
  base64: string;
  prompt: string;
  timestamp: number;
}

/**
 * Standalone window showing all AI-generated images from the current session.
 * Receives image data via Tauri events from the main app.
 * Click any thumbnail to view full-size with save/print.
 */
export default function ImageHistoryWindow() {
  const [images, setImages] = useState<HistoryImage[]>([]);
  const [selected, setSelected] = useState<HistoryImage | null>(null);
  const [saved, setSaved] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    let unlistenBatch: (() => void) | undefined;
    let unlistenNew: (() => void) | undefined;

    // Initial batch of images sent on window open
    listen<{ images: HistoryImage[] }>('image_history_data', (event) => {
      if (mounted.current) setImages(event.payload.images);
    }).then(fn => { if (mounted.current) unlistenBatch = fn; else fn(); });

    // Live updates when new images are generated
    listen<HistoryImage>('image_history_new', (event) => {
      if (mounted.current) setImages(prev => [...prev, event.payload]);
    }).then(fn => { if (mounted.current) unlistenNew = fn; else fn(); });

    return () => {
      mounted.current = false;
      unlistenBatch?.();
      unlistenNew?.();
    };
  }, []);

  const handleSave = useCallback(() => {
    if (!selected) return;
    downloadBase64Image(selected.base64, 'illustration');
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [selected]);

  const handlePrint = useCallback(() => { window.print(); }, []);
  const handleClose = useCallback(() => {
    getCurrentWebviewWindow().close().catch((e) => {
      console.error('Failed to close image history window:', e);
    });
  }, []);

  return (
    <div className="img-hist-container">
      <div className="img-hist-toolbar">
        <span className="img-hist-title">Image History ({images.length})</span>
        <div className="img-hist-actions">
          {selected && (
            <>
              <button className="img-hist-btn" onClick={handleSave}>{saved ? 'Saved' : 'Save'}</button>
              <button className="img-hist-btn" onClick={handlePrint}>Print</button>
              <button className="img-hist-btn" onClick={() => setSelected(null)}>Back</button>
            </>
          )}
          <button className="img-hist-btn img-hist-btn-close" onClick={handleClose}>Close</button>
        </div>
      </div>

      {selected ? (
        <div className="img-hist-detail">
          <img
            src={`data:image/png;base64,${selected.base64}`}
            alt={selected.prompt}
            className="img-hist-detail-img"
          />
          <div className="img-hist-detail-prompt">{selected.prompt}</div>
          <div className="img-hist-detail-time">
            {new Date(selected.timestamp).toLocaleTimeString()}
          </div>
        </div>
      ) : images.length === 0 ? (
        <div className="img-hist-empty">No images generated yet</div>
      ) : (
        <div className="img-hist-grid">
          {images.map((img, i) => (
            <div key={i} className="img-hist-thumb" onClick={() => setSelected(img)}>
              <img src={`data:image/png;base64,${img.base64}`} alt={img.prompt} />
              <div className="img-hist-thumb-label">{img.prompt}</div>
              <div className="img-hist-thumb-time">{new Date(img.timestamp).toLocaleTimeString()}</div>
            </div>
          ))}
        </div>
      )}

      <style>{`
        .img-hist-container {
          width: 100vw; height: 100vh;
          display: flex; flex-direction: column;
          background: #111827; color: #e5e7eb;
          overflow: hidden; font-family: -apple-system, BlinkMacSystemFont, sans-serif;
        }
        .img-hist-toolbar {
          display: flex; align-items: center; justify-content: space-between;
          padding: 8px 12px; background: #0f172a; border-bottom: 1px solid #374151;
          flex-shrink: 0;
        }
        .img-hist-title { font-size: 13px; font-weight: 600; color: #9ca3af; }
        .img-hist-actions { display: flex; gap: 6px; }
        .img-hist-btn {
          padding: 4px 12px; font-size: 12px; font-weight: 500;
          border: 1px solid #4b5563; border-radius: 5px;
          background: #1f2937; color: #e5e7eb; cursor: pointer;
        }
        .img-hist-btn:hover { background: #374151; }
        .img-hist-btn-close { border-color: #6b7280; }
        .img-hist-empty {
          flex: 1; display: flex; align-items: center; justify-content: center;
          color: #6b7280; font-size: 14px;
        }
        .img-hist-grid {
          flex: 1; overflow-y: auto; padding: 12px;
          display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
          gap: 12px; align-content: start;
        }
        .img-hist-thumb {
          border: 1px solid #374151; border-radius: 8px; overflow: hidden;
          cursor: pointer; background: #1f2937; transition: border-color 0.15s;
        }
        .img-hist-thumb:hover { border-color: #60a5fa; }
        .img-hist-thumb img { width: 100%; height: auto; display: block; }
        .img-hist-thumb-label {
          padding: 4px 8px; font-size: 11px; color: #9ca3af;
          white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
        }
        .img-hist-thumb-time {
          padding: 0 8px 6px; font-size: 10px; color: #6b7280;
        }
        .img-hist-detail {
          flex: 1; display: flex; flex-direction: column;
          align-items: center; justify-content: center; padding: 16px; gap: 8px;
          overflow: hidden;
        }
        .img-hist-detail-img { max-width: 100%; max-height: calc(100vh - 120px); object-fit: contain; }
        .img-hist-detail-prompt { font-size: 13px; color: #9ca3af; text-align: center; }
        .img-hist-detail-time { font-size: 11px; color: #6b7280; }
        @media print {
          .img-hist-toolbar, .img-hist-grid { display: none !important; }
          .img-hist-container { background: #fff; }
          .img-hist-detail-img { max-width: 100%; max-height: 100vh; }
          @page { margin: 0.5in; }
        }
      `}</style>
    </div>
  );
}
