import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { MedEntry } from '../types';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  isLoading?: boolean;
  toolsUsed?: string[];
}

export interface UseClinicalChatResult {
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;
  sendMessage: (content: string) => Promise<void>;
  clearChat: () => void;
}

// Response from the Rust backend
interface ClinicalChatResponse {
  content: string;
  tools_used: string[];
}

const SYSTEM_PROMPT = `You are a clinical reference tool used by a licensed physician during patient appointments. Respond as you would to a medical colleague: concise, evidence-based, no disclaimers. The physician will apply clinical judgment.`;

/**
 * Patient identity passed alongside each chat message. The Rust backend
 * formats it as a single system message ("Patient: <name> (DOB <dob>,
 * age <age>)"), omitting any null fields. All three fields are optional.
 */
export interface ClinicalChatPatient {
  name: string | null;
  dob: string | null;
  age: number | null;
}

/**
 * Hook for the Clinical Assistant chat.
 *
 * Per-message context: the hook reads the LATEST `currentMedications`,
 * `clinicalContext`, and `patient` on each `sendMessage` call (via refs), so
 * mid-conversation edits in the sidebar flow through to the next LLM turn
 * without needing to clear chat history. The Rust `clinical_chat_send`
 * command builds system messages from each non-empty piece of context;
 * passing `null`/`undefined` leaves that slot off.
 */
export function useClinicalChat(
  llmRouterUrl: string,
  llmApiKey: string,
  llmClientId: string,
  currentMedications?: MedEntry[] | null,
  clinicalContext?: string | null,
  patient?: ClinicalChatPatient | null
): UseClinicalChatResult {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cancelledRef = useRef(false);
  const sendingRef = useRef(false);

  // Ref tracks latest messages so sendMessage doesn't need messages in its deps.
  // This keeps sendMessage stable across renders, preventing downstream re-renders.
  const messagesRef = useRef<ChatMessage[]>([]);
  messagesRef.current = messages;

  // Same trick for currentMedications: keep sendMessage stable regardless
  // of how often the medication list updates.
  const medsRef = useRef<MedEntry[] | null>(currentMedications ?? null);
  medsRef.current = currentMedications ?? null;

  // Same trick for clinicalContext + patient — refs reassigned on each
  // render keep `sendMessage` stable but pick up the LIVE values when the
  // clinician fires the next message.
  const clinicalContextRef = useRef<string | null>(clinicalContext ?? null);
  clinicalContextRef.current = clinicalContext ?? null;

  const patientRef = useRef<ClinicalChatPatient | null>(patient ?? null);
  patientRef.current = patient ?? null;

  const sendMessage = useCallback(async (content: string) => {
    if (!content.trim() || !llmRouterUrl) {
      setError('Please configure LLM Router URL in settings');
      return;
    }

    if (sendingRef.current) {
      console.warn('Clinical chat: send already in progress, skipping');
      return;
    }

    // Reset cancelled flag
    cancelledRef.current = false;
    sendingRef.current = true;

    const userMessage: ChatMessage = {
      id: `user-${Date.now()}`,
      role: 'user',
      content: content.trim(),
      timestamp: Date.now(),
    };

    // Add user message and placeholder for assistant response
    const loadingMessage: ChatMessage = {
      id: `assistant-${Date.now()}`,
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      isLoading: true,
    };

    setMessages(prev => [...prev, userMessage, loadingMessage]);
    setIsLoading(true);
    setError(null);

    try {
      // Build conversation history from ref (avoids stale closure, keeps sendMessage stable)
      const apiMessages = [
        { role: 'system', content: SYSTEM_PROMPT },
        ...messagesRef.current.filter(m => !m.isLoading).map(m => ({
          role: m.role,
          content: m.content,
        })),
        { role: 'user', content: content.trim() },
      ];

      // Use Tauri invoke instead of browser fetch. Backend collapses
      // empty / all-None context to no system message, so no frontend guard.
      const meds = medsRef.current && medsRef.current.length > 0 ? medsRef.current : null;
      const chartContext = clinicalContextRef.current?.trim() || null;
      const response = await invoke<ClinicalChatResponse>('clinical_chat_send', {
        llmRouterUrl,
        llmApiKey,
        llmClientId,
        messages: apiMessages,
        currentMedications: meds,
        currentPatient: patientRef.current,
        chartContext,
      });

      // Check if cancelled during the request
      if (cancelledRef.current) {
        setMessages(prev => prev.filter(m => m.id !== loadingMessage.id));
        return;
      }

      // Update the loading message with actual response
      setMessages(prev => prev.map(m =>
        m.id === loadingMessage.id
          ? {
              ...m,
              content: response.content,
              isLoading: false,
              toolsUsed: response.tools_used.length > 0 ? response.tools_used : undefined,
            }
          : m
      ));

    } catch (err) {
      // Check if cancelled during the request
      if (cancelledRef.current) {
        setMessages(prev => prev.filter(m => m.id !== loadingMessage.id));
        return;
      }

      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error('Clinical chat error:', errorMessage);
      setError(errorMessage);

      // Update loading message to show error
      setMessages(prev => prev.map(m =>
        m.id === loadingMessage.id
          ? {
              ...m,
              content: `Error: ${errorMessage}`,
              isLoading: false,
            }
          : m
      ));
    } finally {
      sendingRef.current = false;
      setIsLoading(false);
    }
  }, [llmRouterUrl, llmApiKey, llmClientId]);

  const clearChat = useCallback(() => {
    // Mark any pending request as cancelled
    cancelledRef.current = true;
    sendingRef.current = false;
    setMessages([]);
    setError(null);
    setIsLoading(false);
  }, []);

  return {
    messages,
    isLoading,
    error,
    sendMessage,
    clearChat,
  };
}
