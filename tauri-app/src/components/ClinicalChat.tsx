import { memo, useState, useRef, useEffect, useCallback } from 'react';
import type { ChatMessage } from '../hooks/useClinicalChat';

interface ClinicalChatProps {
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;
  onSendMessage: (content: string) => void;
  onClear: () => void;
  isExpanded: boolean;
  onToggleExpand: () => void;
}

/**
 * Clinical Chat component - allows clinician to ask questions during recording
 */
export const ClinicalChat = memo(function ClinicalChat({
  messages,
  isLoading,
  error,
  onSendMessage,
  onClear,
  isExpanded,
  onToggleExpand,
}: ClinicalChatProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (isExpanded && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, isExpanded]);

  // Focus input when expanded
  useEffect(() => {
    if (isExpanded && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isExpanded]);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (input.trim() && !isLoading) {
      onSendMessage(input.trim());
      setInput('');
    }
  }, [input, isLoading, onSendMessage]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  }, [handleSubmit]);

  return (
    <div className={`clinical-chat ${isExpanded ? 'expanded' : 'collapsed'}`}>
      {/* Header - always visible */}
      <button
        className="clinical-chat-header"
        onClick={onToggleExpand}
        aria-expanded={isExpanded}
        aria-label={isExpanded ? 'Collapse clinical assistant' : 'Expand clinical assistant'}
      >
        <span className="chat-header-icon">💬</span>
        <span className="chat-header-title">Clinical Assistant</span>
        <span className="chat-header-chevron">{isExpanded ? '▼' : '▲'}</span>
      </button>

      {/* Chat content - only shown when expanded */}
      {isExpanded && (
        <div className="clinical-chat-content">
          {/* Messages area */}
          <div className="chat-messages">
            {messages.length === 0 ? (
              <div className="chat-empty">
                <div className="chat-empty-icon">🔍</div>
                <div className="chat-empty-text">
                  Ask medical questions during the appointment
                </div>
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
                    <div className="message-content user-message">
                      {message.content}
                    </div>
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
                          {message.content}
                          {message.toolsUsed && message.toolsUsed.length > 0 && (
                            <div className="message-tools">
                              🌐 Searched web
                            </div>
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

          {/* Error display */}
          {error && (
            <div className="chat-error">
              {error}
            </div>
          )}

          {/* Input area */}
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

          {/* Clear button */}
          {messages.length > 0 && (
            <button
              className="chat-clear-button"
              onClick={onClear}
              aria-label="Clear chat history"
            >
              Clear chat
            </button>
          )}
        </div>
      )}
    </div>
  );
});

export default ClinicalChat;
