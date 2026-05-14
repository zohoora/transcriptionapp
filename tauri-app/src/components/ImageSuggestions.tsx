import { memo, useCallback } from 'react';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { emitTo } from '@tauri-apps/api/event';
import type { AiImage } from '../hooks/useAiImages';
import type { ImageSource } from '../types';

interface ImageSuggestionsProps {
  aiImages: AiImage[];
  aiLoading: boolean;
  aiError: string | null;
  onAiGenerate: (description: string) => void;
  onAiDismiss: (index: number) => void;
  imageSource: ImageSource;
  /** Provider+quality key: "gemini-flash"|"gemini-pro"|"openai-low"|"openai-medium"|"openai-high". */
  imageModel?: string;
  onImageModelChange?: (value: string) => void;
}

type Provider = 'gemini' | 'openai';
type GeminiQuality = 'flash' | 'pro';
type OpenAIQuality = 'low' | 'medium' | 'high';

/** Split a combined model key into provider + quality halves. */
function splitModelKey(key: string): { provider: Provider; quality: string } {
  if (key.startsWith('openai-')) {
    return { provider: 'openai', quality: key.slice('openai-'.length) };
  }
  // Default to gemini for any unrecognized shape.
  return {
    provider: 'gemini',
    quality: key.startsWith('gemini-') ? key.slice('gemini-'.length) : 'flash',
  };
}

/** Re-join provider + quality into the flat key used by the backend. */
function joinModelKey(provider: Provider, quality: string): string {
  return `${provider}-${quality}`;
}

/**
 * Patient Illustration panel: AI-generated medical illustrations via Gemini
 * or OpenAI. The clinician types a description; the selected model+quality
 * produces an image. Renders nothing when `imageSource === 'off'`.
 */
export const ImageSuggestions = memo(function ImageSuggestions({
  aiImages,
  aiLoading,
  aiError,
  onAiGenerate,
  onAiDismiss,
  imageSource,
  imageModel = 'gemini-flash',
  onImageModelChange,
}: ImageSuggestionsProps) {
  const { provider, quality } = splitModelKey(imageModel);

  // Open AI image in a separate resizable window
  const openAiImageWindow = useCallback(async (img: AiImage) => {
    try {
      const existing = await WebviewWindow.getByLabel('image-viewer');
      if (existing) {
        await existing.close();
      }

      const viewer = new WebviewWindow('image-viewer', {
        url: 'image-viewer.html',
        title: 'Medical Illustration',
        width: 800,
        height: 700,
        minWidth: 300,
        minHeight: 250,
        resizable: true,
      });

      // Send image data after window loads
      viewer.once('tauri://webview-created', async () => {
        // Small delay to let React mount
        setTimeout(async () => {
          try {
            await emitTo('image-viewer', 'image_viewer_data', { base64: img.base64, prompt: img.prompt });
          } catch (e) {
            console.error('Failed to send image data:', e);
          }
        }, 300);
      });

      viewer.once('tauri://error', (e) => {
        console.error('Failed to open image viewer:', e);
      });
    } catch (e) {
      console.error('Error opening image viewer:', e);
    }
  }, []);

  const openImageHistory = useCallback(async () => {
    if (aiImages.length === 0) return;
    try {
      const existing = await WebviewWindow.getByLabel('image-history');
      if (existing) { await existing.setFocus(); return; }

      const win = new WebviewWindow('image-history', {
        url: 'image-history.html',
        title: 'Image History',
        width: 700,
        height: 600,
        minWidth: 400,
        minHeight: 300,
        resizable: true,
      });
      win.once('tauri://webview-created', () => {
        setTimeout(async () => {
          try {
            await emitTo('image-history', 'image_history_data', {
              images: aiImages.map(img => ({ base64: img.base64, prompt: img.prompt, timestamp: img.timestamp })),
            });
          } catch (e) { console.error('Failed to send history data:', e); }
        }, 300);
      });
    } catch (e) { console.error('Error opening image history:', e); }
  }, [aiImages]);

  if (imageSource !== 'ai') {
    return null;
  }

  const handleProviderChange = (next: Provider) => {
    if (!onImageModelChange) return;
    // Reset quality to each provider's sensible default on switch.
    const nextQuality: GeminiQuality | OpenAIQuality =
      next === 'gemini' ? 'flash' : 'low';
    onImageModelChange(joinModelKey(next, nextQuality));
  };
  const handleQualityChange = (next: string) => {
    if (!onImageModelChange) return;
    onImageModelChange(joinModelKey(provider, next));
  };

  return (
    <div className="ai-image-section">
      <div className="ai-image-header">
        <span className="ai-image-title">Patient Illustration</span>
      </div>

      <div className="ai-image-model-picker">
        <label className="ai-image-model-label">
          Model
          <select
            className="ai-image-model-select"
            value={provider}
            onChange={(e) => handleProviderChange(e.target.value as Provider)}
            disabled={aiLoading || !onImageModelChange}
          >
            <option value="gemini">Gemini</option>
            <option value="openai">OpenAI</option>
          </select>
        </label>
        <label className="ai-image-model-label">
          Quality
          <select
            className="ai-image-model-select"
            value={quality}
            onChange={(e) => handleQualityChange(e.target.value)}
            disabled={aiLoading || !onImageModelChange}
          >
            {provider === 'gemini' ? (
              <>
                <option value="flash">Flash</option>
                <option value="pro">Pro</option>
              </>
            ) : (
              <>
                <option value="low">Low</option>
                <option value="medium">Medium</option>
                <option value="high">High</option>
              </>
            )}
          </select>
        </label>
      </div>

      {/* Prompt input */}
      <form className="ai-image-form" onSubmit={e => {
        e.preventDefault();
        const input = e.currentTarget.elements.namedItem('ai-prompt') as HTMLTextAreaElement;
        if (input.value.trim()) {
          onAiGenerate(input.value);
          input.value = '';
        }
      }}>
        <textarea
          name="ai-prompt"
          className="ai-image-input"
          placeholder="Describe what to illustrate (e.g., knee joint anatomy, lumbar disc herniation, insulin injection sites...)"
          rows={2}
          disabled={aiLoading}
        />
        <button
          type="submit"
          className="ai-image-generate-btn"
          disabled={aiLoading}
        >
          {aiLoading ? 'Generating...' : 'Generate'}
        </button>
      </form>

      {aiError && (
        <div className="ai-image-error">{aiError}</div>
      )}

      {/* Display latest image */}
      {aiImages.length > 0 && (() => {
        const img = aiImages[aiImages.length - 1];
        const index = aiImages.length - 1;
        return (
          <div className="ai-image-result" onClick={() => openAiImageWindow(img)}>
            <img
              src={`data:image/png;base64,${img.base64}`}
              alt="AI-generated medical illustration"
              className="ai-image-img"
            />
            <div className="ai-image-prompt-label">{img.prompt}</div>
            <button
              className="ai-image-dismiss"
              onClick={e => { e.stopPropagation(); onAiDismiss(index); }}
              title="Dismiss"
            >
              ×
            </button>
          </div>
        );
      })()}

      {aiImages.length > 1 && (
        <button className="ai-image-history-link" onClick={openImageHistory}>
          View all {aiImages.length} images
        </button>
      )}
    </div>
  );
});
