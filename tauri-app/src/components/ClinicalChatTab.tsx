import { memo, useState, useRef, useEffect, useCallback } from 'react';
import { MarkdownContent } from './ClinicalChat';
import type { ChatMessage } from '../hooks/useClinicalChat';

interface ClinicalChatTabProps {
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;
  onSendMessage: (content: string) => void;
  onClear: () => void;
}

/**
 * Clinical Assistant chat — tab variant for the standalone window
 * (no expand/collapse; the window itself is the container). The sidebar
 * shows attached med context, so this tab no longer renders its own banner.
 */
export const ClinicalChatTab = memo(function ClinicalChatTab({
  messages,
  isLoading,
  error,
  onSendMessage,
  onClear,
}: ClinicalChatTabProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      if (input.trim() && !isLoading) {
        onSendMessage(input.trim());
        setInput('');
      }
    },
    [input, isLoading, onSendMessage]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSubmit(e);
      }
    },
    [handleSubmit]
  );

  return (
    <div className="clinical-chat-tab">
      <div className="chat-messages">
        {messages.length === 0 ? (
          <div className="chat-empty">
            <div className="chat-empty-icon">🔍</div>
            <div className="chat-empty-text">Ask medical questions during the appointment</div>
            <div className="chat-empty-examples">
              <span>Try:</span> "Lisinopril dosage?" or "Warfarin interactions?"
            </div>
          </div>
        ) : (
          messages.map((message) => (
            <div
              key={message.id}
              className={`chat-message ${message.role} ${message.isLoading ? 'loading' : ''}`}
            >
              {message.role === 'user' ? (
                <div className="message-content user-message">{message.content}</div>
              ) : (
                <div className="message-content assistant-message">
                  {message.isLoading ? (
                    <div className="message-loading">
                      <span className="loading-dot" />
                      <span className="loading-dot" />
                      <span className="loading-dot" />
                    </div>
                  ) : (
                    <>
                      <MarkdownContent content={message.content} />
                      {message.toolsUsed && message.toolsUsed.length > 0 && (
                        <div className="message-tools">🌐 Searched web</div>
                      )}
                    </>
                  )}
                </div>
              )}
            </div>
          ))
        )}
        <div ref={messagesEndRef} />
      </div>

      {error && <div className="chat-error">{error}</div>}

      <form className="chat-input-form" onSubmit={handleSubmit}>
        <input
          ref={inputRef}
          type="text"
          className="chat-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Ask a question..."
          disabled={isLoading}
          aria-label="Chat message input"
        />
        <button
          type="submit"
          className="chat-send-button"
          disabled={!input.trim() || isLoading}
          aria-label="Send message"
        >
          {isLoading ? '...' : '→'}
        </button>
      </form>

      {messages.length > 0 && (
        <button className="chat-clear-button" onClick={onClear} aria-label="Clear chat history">
          Clear chat
        </button>
      )}
    </div>
  );
});

export default ClinicalChatTab;
