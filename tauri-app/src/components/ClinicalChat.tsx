import { memo, useState, useRef, useEffect, useCallback, useMemo } from 'react';
import type { ChatMessage } from '../hooks/useClinicalChat';

/**
 * Simple markdown parser for chat messages
 * Supports: bold, italic, code, code blocks, lists, headers, links
 * Note: Content is sanitized by escaping HTML entities before parsing
 */
function parseMarkdown(text: string): string {
  let html = text;

  // Escape HTML entities first to prevent XSS
  html = html
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  // Code blocks (```code```) - must be before inline code
  html = html.replace(/```(\w*)\n?([\s\S]*?)```/g, (_match, _lang, code) => {
    return `<pre><code>${code.trim()}</code></pre>`;
  });

  // Inline code (`code`)
  html = html.replace(/`([^`]+)`/g, '<code>$1</code>');

  // Bold (**text** or __text__)
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/__([^_]+)__/g, '<strong>$1</strong>');

  // Italic (*text* or _text_) - be careful not to match bold
  html = html.replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, '<em>$1</em>');
  html = html.replace(/(?<!_)_([^_]+)_(?!_)/g, '<em>$1</em>');

  // Headers (## Header)
  html = html.replace(/^#### (.+)$/gm, '<h4>$1</h4>');
  html = html.replace(/^### (.+)$/gm, '<h3>$1</h3>');
  html = html.replace(/^## (.+)$/gm, '<h2>$1</h2>');
  html = html.replace(/^# (.+)$/gm, '<h1>$1</h1>');

  // Horizontal rule
  html = html.replace(/^---$/gm, '<hr>');

  // Unordered lists (- item or * item)
  html = html.replace(/^[\-\*] (.+)$/gm, '<li>$1</li>');

  // Ordered lists (1. item)
  html = html.replace(/^\d+\. (.+)$/gm, '<li>$1</li>');

  // Wrap consecutive <li> elements in <ul>
  html = html.replace(/(<li>.*<\/li>\n?)+/g, (match) => `<ul>${match}</ul>`);

  // Links [text](url)
  html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');

  // Blockquotes (> text)
  html = html.replace(/^&gt; (.+)$/gm, '<blockquote>$1</blockquote>');

  // Paragraphs - wrap text blocks not already in tags
  // Split by double newlines for paragraphs
  const lines = html.split(/\n\n+/);
  html = lines.map(line => {
    const trimmed = line.trim();
    if (!trimmed) return '';
    // Don't wrap if already has block-level tags
    if (/^<(h[1-4]|ul|ol|li|pre|blockquote|hr|p)/.test(trimmed)) {
      return trimmed;
    }
    // Replace single newlines with <br> within paragraphs
    return `<p>${trimmed.replace(/\n/g, '<br>')}</p>`;
  }).join('\n');

  return html;
}

/**
 * Component to render markdown content safely
 * HTML is sanitized by escaping all HTML entities before markdown parsing
 */
function MarkdownContent({ content }: { content: string }) {
  const html = useMemo(() => parseMarkdown(content), [content]);

  return (
    <div
      className="markdown-content"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

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
                          <MarkdownContent content={message.content} />
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
