import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

const COOLDOWN_MS = 45000; // 45 seconds between generations
const SESSION_CAP = 8; // Max images per session

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
  cooldownRemaining: number;
  sessionCount: number;
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
  const [cooldownRemaining, setCooldownRemaining] = useState(0);

  const lastGenerationTime = useRef(0);
  const sessionCountRef = useRef(0);
  const [sessionCount, setSessionCount] = useState(0);
  const isGenerating = useRef(false);

  // Reset on session change
  useEffect(() => {
    setImages([]);
    setError(null);
    setCooldownRemaining(0);
    lastGenerationTime.current = 0;
    sessionCountRef.current = 0;
    setSessionCount(0);
    isGenerating.current = false;
  }, [sessionId]);

  // Cooldown countdown timer
  useEffect(() => {
    if (cooldownRemaining <= 0) return;
    const timer = setInterval(() => {
      const elapsed = Date.now() - lastGenerationTime.current;
      const remaining = Math.max(0, COOLDOWN_MS - elapsed);
      setCooldownRemaining(remaining);
      if (remaining <= 0) clearInterval(timer);
    }, 1000);
    return () => clearInterval(timer);
  }, [cooldownRemaining]);

  const generate = useCallback((description: string) => {
    if (!enabled || !sessionId) return;
    if (!description.trim()) return;
    if (sessionCountRef.current >= SESSION_CAP) return;
    if (isGenerating.current) return;

    const now = Date.now();
    if (now - lastGenerationTime.current < COOLDOWN_MS) return;

    isGenerating.current = true;
    lastGenerationTime.current = now;
    setCooldownRemaining(COOLDOWN_MS);
    setIsLoading(true);
    setError(null);

    const fullPrompt = IMAGE_PROMPT_PREFIX + description.trim();

    invoke<{ imageBase64: string; prompt: string }>('generate_ai_image', {
      prompt: fullPrompt,
    })
      .then((result) => {
        sessionCountRef.current += 1;
        setSessionCount(sessionCountRef.current);
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

  return { images, isLoading, error, cooldownRemaining, sessionCount, generate, dismissImage };
}
