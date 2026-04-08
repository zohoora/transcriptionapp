import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface AiImage {
  base64: string;
  prompt: string;
  timestamp: number;
}

interface UseAiImagesOptions {
  enabled: boolean; // image_source === "ai"
  sessionId: string | null;
}

interface UseAiImagesResult {
  images: AiImage[];
  isLoading: boolean;
  error: string | null;
  generate: (description: string) => void;
  dismissImage: (index: number) => void;
}

const IMAGE_PROMPT_PREFIX =
  'Generate a clear, simple medical illustration that would help a clinician explain the following to a patient. Focus on anatomical accuracy and patient-friendly visuals. Description: ';

export function useAiImages({
  enabled,
  sessionId,
}: UseAiImagesOptions): UseAiImagesResult {
  const [images, setImages] = useState<AiImage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isGenerating = useRef(false);

  // Reset on session change
  useEffect(() => {
    setImages([]);
    setError(null);
    isGenerating.current = false;
  }, [sessionId]);

  const generate = useCallback((description: string) => {
    if (!enabled || !sessionId) return;
    if (!description.trim()) return;
    if (isGenerating.current) return;

    isGenerating.current = true;
    setIsLoading(true);
    setError(null);

    const fullPrompt = IMAGE_PROMPT_PREFIX + description.trim();

    invoke<{ imageBase64: string; prompt: string }>('generate_ai_image', {
      prompt: fullPrompt,
    })
      .then((result) => {
        setImages((prev) => [
          ...prev,
          {
            base64: result.imageBase64,
            prompt: description.trim(),
            timestamp: Date.now(),
          },
        ]);
      })
      .catch((e) => {
        setError(String(e));
      })
      .finally(() => {
        setIsLoading(false);
        isGenerating.current = false;
      });
  }, [enabled, sessionId]);

  const dismissImage = useCallback((index: number) => {
    setImages((prev) => prev.filter((_, i) => i !== index));
  }, []);

  return { images, isLoading, error, generate, dismissImage };
}
