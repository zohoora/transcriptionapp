import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ImageConcept } from './usePredictiveHint';

/** MIIS suggested image */
export interface MiisSuggestion {
  image_id: number;
  score: number;
  sha256: string;
  title: string | null;
  description: string | null;
  thumb_url: string;
  display_url: string;
}

/** MIIS suggest response */
interface SuggestResponse {
  suggestions: MiisSuggestion[];
  suggestion_set_id: string;
}

/** Telemetry event types */
type EventType = 'impression' | 'click' | 'open_full' | 'print' | 'dismiss';

interface UsageEvent {
  image_id: number;
  event_type: EventType;
  timestamp: string;
}

interface UseMiisImagesOptions {
  /** Session ID for the current recording */
  sessionId: string | null;
  /** Medical concepts from predictive hint */
  concepts: ImageConcept[];
  /** Whether MIIS is enabled in settings */
  enabled: boolean;
  /** MIIS server URL */
  serverUrl: string;
}

interface UseMiisImagesResult {
  /** Current image suggestions */
  suggestions: MiisSuggestion[];
  /** Current suggestion set ID (for telemetry) */
  suggestionSetId: string | null;
  /** Whether suggestions are loading */
  isLoading: boolean;
  /** Error message if any */
  error: string | null;
  /** Record an impression event */
  recordImpression: (imageId: number) => void;
  /** Record a click event */
  recordClick: (imageId: number) => void;
  /** Record a dismiss event */
  recordDismiss: (imageId: number) => void;
  /** Get full image URL */
  getImageUrl: (path: string) => string;
}

const TELEMETRY_FLUSH_INTERVAL = 5000; // 5 seconds

/**
 * Hook to fetch and manage MIIS image suggestions during recording.
 * Uses concepts from predictive hints to fetch relevant medical illustrations.
 */
export function useMiisImages({
  sessionId,
  concepts,
  enabled,
  serverUrl,
}: UseMiisImagesOptions): UseMiisImagesResult {
  const [suggestions, setSuggestions] = useState<MiisSuggestion[]>([]);
  const [suggestionSetId, setSuggestionSetId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Telemetry queue
  const telemetryQueue = useRef<UsageEvent[]>([]);
  const lastConceptsRef = useRef<string>('');

  // Get full URL for an image path
  const getImageUrl = useCallback(
    (path: string) => {
      if (!serverUrl) return '';
      // Path should be like /api/assets/... but MIIS uses /v5/...
      return `${serverUrl}${path}`;
    },
    [serverUrl]
  );

  // Record telemetry event
  const recordEvent = useCallback((imageId: number, eventType: EventType) => {
    telemetryQueue.current.push({
      image_id: imageId,
      event_type: eventType,
      timestamp: new Date().toISOString(),
    });
  }, []);

  const recordImpression = useCallback(
    (imageId: number) => recordEvent(imageId, 'impression'),
    [recordEvent]
  );

  const recordClick = useCallback(
    (imageId: number) => recordEvent(imageId, 'click'),
    [recordEvent]
  );

  const recordDismiss = useCallback(
    (imageId: number) => recordEvent(imageId, 'dismiss'),
    [recordEvent]
  );

  // Flush telemetry to server
  const flushTelemetry = useCallback(async () => {
    if (!sessionId || !serverUrl || telemetryQueue.current.length === 0) return;

    const events = telemetryQueue.current.splice(0);
    try {
      // Use Tauri command to proxy the request (avoids CORS)
      await invoke('miis_send_usage', {
        serverUrl,
        sessionId,
        suggestionSetId,
        events,
      });
    } catch (e) {
      console.error('Failed to send MIIS telemetry:', e);
      // Put events back in queue for retry
      telemetryQueue.current.unshift(...events);
    }
  }, [sessionId, serverUrl, suggestionSetId]);

  // Fetch suggestions when concepts change
  useEffect(() => {
    if (!enabled || !sessionId || !serverUrl || concepts.length === 0) {
      return;
    }

    // Check if concepts have meaningfully changed
    const conceptsKey = concepts.map((c) => `${c.text}:${c.weight}`).join('|');
    if (conceptsKey === lastConceptsRef.current) {
      return;
    }
    lastConceptsRef.current = conceptsKey;

    const fetchSuggestions = async () => {
      setIsLoading(true);
      setError(null);

      try {
        // Use Tauri command to proxy the request (avoids CORS)
        const response = await invoke<SuggestResponse>('miis_suggest', {
          serverUrl,
          sessionId,
          concepts: concepts.slice(0, 5), // Max 5 concepts
        });

        setSuggestions(response.suggestions);
        setSuggestionSetId(response.suggestion_set_id);
      } catch (e) {
        const errorMsg = e instanceof Error ? e.message : String(e);
        console.error('Failed to fetch MIIS suggestions:', errorMsg);
        setError(errorMsg);
      } finally {
        setIsLoading(false);
      }
    };

    fetchSuggestions();
  }, [enabled, sessionId, serverUrl, concepts]);

  // Set up telemetry flush interval
  useEffect(() => {
    if (!enabled || !sessionId) return;

    const interval = setInterval(flushTelemetry, TELEMETRY_FLUSH_INTERVAL);

    // Flush on unmount
    return () => {
      clearInterval(interval);
      flushTelemetry();
    };
  }, [enabled, sessionId, flushTelemetry]);

  // Clear state when session ends
  useEffect(() => {
    if (!sessionId) {
      setSuggestions([]);
      setSuggestionSetId(null);
      lastConceptsRef.current = '';
    }
  }, [sessionId]);

  return {
    suggestions,
    suggestionSetId,
    isLoading,
    error,
    recordImpression,
    recordClick,
    recordDismiss,
    getImageUrl,
  };
}
