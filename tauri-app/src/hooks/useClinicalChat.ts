import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

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

const SYSTEM_PROMPT = `You are a clinical assistant helping during a patient appointment. Provide concise, accurate medical information. If you're uncertain about something, say so. When looking up information, mention that you searched for it.`;

export function useClinicalChat(
  llmRouterUrl: string,
  llmApiKey: string,
  llmClientId: string
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

  const sendMessage = useCallback(async (content: string) => {
    console.log('Clinical chat: sending message via Tauri invoke', { llmRouterUrl, llmApiKey: llmApiKey ? '***' : 'empty', llmClientId });

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

      // Use Tauri invoke instead of browser fetch
      const response = await invoke<ClinicalChatResponse>('clinical_chat_send', {
        llmRouterUrl,
        llmApiKey,
        llmClientId,
        messages: apiMessages,
      });

      console.log('Clinical chat: received response', { contentLength: response.content.length, toolsUsed: response.tools_used });

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
