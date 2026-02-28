import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

const COOLDOWN_MS = 45000; // 45 seconds between generations
const SESSION_CAP = 8; // Max images per session
const MAX_VISIBLE = 6; // Max images shown at once

export interface AiImage {
  base64: string;
  prompt: string;
  timestamp: number;
}

interface UseAiImagesOptions {
  imagePrompt: string | null;
  enabled: boolean; // image_source === "ai"
  sessionId: string | null;
}

interface UseAiImagesResult {
  images: AiImage[];
  isLoading: boolean;
  error: string | null;
  dismissImage: (index: number) => void;
}

export function useAiImages({
  imagePrompt,
  enabled,
  sessionId,
}: UseAiImagesOptions): UseAiImagesResult {
  const [images, setImages] = useState<AiImage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const lastGenerationTime = useRef(0);
  const lastPrompt = useRef<string | null>(null);
  const sessionCount = useRef(0);
  const isGenerating = useRef(false);

  // Reset on session change
  useEffect(() => {
    setImages([]);
    setError(null);
    lastGenerationTime.current = 0;
    lastPrompt.current = null;
    sessionCount.current = 0;
    isGenerating.current = false;
  }, [sessionId]);

  // Generate image when prompt changes
  useEffect(() => {
    if (!enabled || !imagePrompt || !sessionId) return;

    // Dedup: skip if same prompt
    if (imagePrompt === lastPrompt.current) return;

    // Session cap
    if (sessionCount.current >= SESSION_CAP) return;

    // Cooldown
    const now = Date.now();
    if (now - lastGenerationTime.current < COOLDOWN_MS) return;

    // Concurrency guard
    if (isGenerating.current) return;

    isGenerating.current = true;
    lastPrompt.current = imagePrompt;
    lastGenerationTime.current = now;
    setIsLoading(true);
    setError(null);

    invoke<{ imageBase64: string; prompt: string }>('generate_ai_image', {
      prompt: imagePrompt,
    })
      .then((result) => {
        sessionCount.current += 1;
        setImages((prev) => {
          const next = [
            ...prev,
            {
              base64: result.imageBase64,
              prompt: result.prompt,
              timestamp: Date.now(),
            },
          ];
          // FIFO cap
          return next.length > MAX_VISIBLE
            ? next.slice(next.length - MAX_VISIBLE)
            : next;
        });
      })
      .catch((e) => {
        setError(String(e));
      })
      .finally(() => {
        setIsLoading(false);
        isGenerating.current = false;
      });
  }, [imagePrompt, enabled, sessionId]);

  const dismissImage = useCallback((index: number) => {
    setImages((prev) => prev.filter((_, i) => i !== index));
  }, []);

  return { images, isLoading, error, dismissImage };
}
