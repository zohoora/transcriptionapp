import { useState, useCallback, useRef } from 'react';

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

interface ChatCompletionResponse {
  id: string;
  choices: Array<{
    message: {
      role: string;
      content: string;
    };
    finish_reason: string;
  }>;
  tool_usage?: {
    rounds: number;
    tools_called: Array<{
      name: string;
      arguments: Record<string, unknown>;
      success: boolean;
    }>;
  };
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
  const abortControllerRef = useRef<AbortController | null>(null);

  const sendMessage = useCallback(async (content: string) => {
    if (!content.trim() || !llmRouterUrl) {
      setError('Please configure LLM Router URL in settings');
      return;
    }

    // Cancel any pending request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    abortControllerRef.current = new AbortController();

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
      // Build conversation history for API
      const apiMessages = [
        { role: 'system', content: SYSTEM_PROMPT },
        ...messages.filter(m => !m.isLoading).map(m => ({
          role: m.role,
          content: m.content,
        })),
        { role: 'user', content: content.trim() },
      ];

      const response = await fetch(`${llmRouterUrl}/v1/chat/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${llmApiKey}`,
          'X-Client-Id': llmClientId || 'ai-scribe',
          'X-Clinic-Task': 'clinical_assistant',
        },
        body: JSON.stringify({
          model: 'clinical-assistant',
          messages: apiMessages,
          max_tokens: 500,
          temperature: 0.3,
        }),
        signal: abortControllerRef.current.signal,
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`API error: ${response.status} - ${errorText}`);
      }

      const data: ChatCompletionResponse = await response.json();
      const assistantContent = data.choices?.[0]?.message?.content || 'No response received';

      // Extract tools used
      const toolsUsed = data.tool_usage?.tools_called?.map(t => t.name) || [];

      // Update the loading message with actual response
      setMessages(prev => prev.map(m =>
        m.id === loadingMessage.id
          ? {
              ...m,
              content: assistantContent,
              isLoading: false,
              toolsUsed: toolsUsed.length > 0 ? toolsUsed : undefined,
            }
          : m
      ));

    } catch (err) {
      if (err instanceof Error && err.name === 'AbortError') {
        // Request was cancelled, remove the loading message
        setMessages(prev => prev.filter(m => m.id !== loadingMessage.id));
        return;
      }

      const errorMessage = err instanceof Error ? err.message : 'Failed to send message';
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
      setIsLoading(false);
      abortControllerRef.current = null;
    }
  }, [llmRouterUrl, llmApiKey, llmClientId, messages]);

  const clearChat = useCallback(() => {
    // Cancel any pending request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
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
