# ADR 0017: Clinical Assistant Chat

## Status
Accepted

## Context
During patient encounters, clinicians often need quick answers to medical questions:
- Drug dosages and interactions
- ICD-10 codes
- Clinical guidelines
- Lab reference ranges

Leaving the recording app to search disrupts workflow and may cause missed audio.

## Decision
Embed a collapsible chat interface in RecordingMode that proxies requests through a dedicated LLM model alias.

### Architecture
```
Frontend (ClinicalChat.tsx)
    │
    │ invoke('clinical_chat_send', {...})
    ▼
Rust Backend (commands/clinical_chat.rs)
    │
    │ reqwest HTTP POST
    ▼
LLM Router (/v1/chat/completions)
    │
    │ model: "clinical-assistant"
    ▼
LLM with tool execution (web search, etc.)
```

### Why Rust Proxy?
Browser fetch to the LLM router fails due to:
- CORS restrictions on cross-origin requests
- CSP (Content Security Policy) in Tauri apps
- Missing headers that browsers auto-block

The Rust backend uses `reqwest` which bypasses all browser restrictions.

### API
```typescript
// Frontend hook
const { messages, isLoading, sendMessage } = useClinicalChat({
  llmRouterUrl: settings.llm_router_url,
  llmApiKey: settings.llm_api_key,
  llmClientId: settings.llm_client_id,
});

// Rust command
async fn clinical_chat_send(
    llm_router_url: String,
    llm_api_key: String,
    llm_client_id: String,
    messages: Vec<ChatMessage>,
) -> Result<ClinicalChatResponse, String>
```

### Router Requirements
The LLM router MUST handle tool execution server-side:
1. Model returns tool call JSON
2. Router executes tool (web search, database lookup)
3. Router feeds result back to model
4. Router returns final response

Without this, raw tool call JSON appears in chat.

### Features
- Collapsible panel (minimizes to header bar)
- Markdown rendering (bold, italic, code, lists, headers)
- "🌐 Searched web" indicator when tools were used
- Auto-scroll to latest message
- Focus input on expand

## Consequences

### Positive
- Clinicians get answers without leaving the app
- No audio missed during quick lookups
- Tool execution enables live web search
- Markdown formatting for readable responses

### Negative
- Requires LLM router with tool execution support
- Adds network dependency during recording
- Chat history not persisted (cleared on session end)

### Files
- `src-tauri/src/commands/clinical_chat.rs` - Rust HTTP proxy
- `src/hooks/useClinicalChat.ts` - React hook
- `src/components/ClinicalChat.tsx` - UI component
- `src/styles.css` - Chat styling (`.clinical-chat`, `.markdown-content`)
