# AI-Generated Medical Images — Design

## Problem

The current MIIS integration serves images from a local library via FTS5 search. Image quality is insufficient for patient education — the primary use case is physicians showing anatomical diagrams to patients during visits.

Google's Nano Banana 2 (`gemini-3.1-flash-image-preview`, released Feb 26 2026) produces medical illustrations of sufficient quality at $0.039/image.

## Approach: Piggyback on Predictive Hint

The existing `generate_predictive_hint` LLM call runs every 30s and extracts medical concepts from the transcript. Extend its prompt to also decide whether an image would help and, if so, craft a generation prompt. This adds zero extra LLM calls — only the image generation ($0.04) is new spend.

Target: 3-5 images per encounter (~$0.12-0.20/encounter).

## Data Flow

```
Recording Session
    │
    ├─ Every 30s: generate_predictive_hint (existing LLM call)
    │   │
    │   │  Extended response:
    │   │  {
    │   │    "hint": "Check ferritin if fatigue >2wks",
    │   │    "concepts": [...],              ← for MIIS (unchanged)
    │   │    "image_prompt": "Anatomical diagram of the knee joint,
    │   │                     medial meniscus tear, anterior view,
    │   │                     medical illustration, labeled"
    │   │    OR
    │   │    "image_prompt": null             ← no image needed
    │   │  }
    │   │
    │   ├─ If image_source="miis": concepts → MIIS server (current)
    │   └─ If image_source="ai":  image_prompt → Nano Banana 2 API
    │       └─ Returns base64 image → displayed in UI strip
    │
    └─ Cooldown: 45s minimum between AI image generations
```

## Backend Changes

### Config (`config.rs`)

- `image_source: String` — `"off"` (default), `"miis"`, `"ai"`
- `gemini_api_key: String` — API key for Gemini
- Backward compat: if `miis_enabled=true` and `image_source="off"`, migrate to `"miis"`

### Extended Predictive Hint (`commands/ollama.rs`)

Extend system prompt with third output field:

```
3. IMAGE_PROMPT: If the conversation involves a specific anatomical structure,
   condition, or procedure that a patient would benefit from seeing illustrated,
   provide a detailed image generation prompt. Otherwise, return null.

RULES for image_prompt:
- Style: "medical illustration, anatomical diagram, labeled, clean white background"
- Be anatomically specific: "anterior view of right knee showing torn ACL"
- Include view angle, relevant structures, and any pathology discussed
- Do NOT generate for: lab values, medications, general wellness, psychological topics
- Do NOT repeat the same subject within a session
- Maximum one image_prompt per response
```

Response struct: add `image_prompt: Option<String>` to `PredictiveHintResponse`.

### Gemini Client (`gemini_client.rs`)

New file. Thin wrapper around Gemini `generateContent` endpoint:
- Model: `gemini-3.1-flash-image-preview`
- Endpoint: `https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-image-preview:generateContent`
- Auth: `x-goog-api-key` header
- Request: text prompt + `responseModalities: ["IMAGE"]` + `imageConfig: { aspectRatio: "4:3" }`
- Response: extract base64 image data from `inlineData`
- Timeout: 15s
- Error handling: map to CommandError, log warning, non-blocking

### Image Generation Command (`commands/images.rs`)

New command: `generate_ai_image(prompt: String) -> Result<AiImageResponse, String>`

```rust
pub struct AiImageResponse {
    pub image_base64: String,
    pub prompt: String,
}
```

## Frontend Changes

### `usePredictiveHint.ts`

- Response type extended: `image_prompt: string | null`
- Exposes `imagePrompt` alongside `hint` and `concepts`

### New: `useAiImages.ts`

- Receives `imagePrompt` from predictive hint
- On non-null prompt:
  - Check cooldown (45s since last) — skip if too soon
  - Check dedup (same prompt text) — skip if identical
  - Check session cap (max 8) — skip if exceeded
  - Call `generate_ai_image` command
  - Append to `generatedImages` array (capped at 6 visible, FIFO)
- Tracks generated subjects in a `Set<string>` for session-level dedup
- Exposes `generatedImages: AiImage[]`

```typescript
interface AiImage {
  base64: string;
  prompt: string;
  timestamp: number;
}
```

### `ImageSuggestions.tsx`

- New prop: `imageSource: "miis" | "ai" | "off"`
- `"miis"`: current behavior (thumbnails from MIIS server URLs)
- `"ai"`: renders base64 `<img>` tags in same strip layout; click to expand; dismiss works locally
- No telemetry calls for AI images

### `SettingsDrawer.tsx`

- Replace `miis_enabled` checkbox with 3-option selector: Off / MIIS Library / AI Generated
- When "AI Generated" selected, show `gemini_api_key` password input

### Mode Components

- `RecordingMode.tsx`, `ContinuousMode.tsx`: pass `imageSource` instead of `miisEnabled`
- Wire `useAiImages` hook alongside `useMiisImages`; `imageSource` determines which is active

## Cost Guardrails

| Guard | Value | Purpose |
|-------|-------|---------|
| Cooldown | 45s min between generations | Limits to ~3-5 per encounter |
| Session cap | 8 images max | Hard ceiling on runaway |
| Subject dedup | Track generated topics | Prevents repeat illustrations |
| LLM gating | `image_prompt: null` | LLM decides relevance |
| Config clamp | Cooldown min 30s | Prevents user misconfiguration |

## Safety

- Gemini has built-in safety filters (medical diagrams = educational, should pass)
- No patient data in prompts — only anatomical/condition terms
- API key stored in `config.json` (same pattern as `llm_api_key`)
- Safety block or error → log warning, continue silently
- All generated images include SynthID watermark (Google standard)

## Files to Create/Modify

| File | Action |
|------|--------|
| `src-tauri/src/gemini_client.rs` | Create — Gemini API client |
| `src-tauri/src/commands/images.rs` | Create — `generate_ai_image` command |
| `src-tauri/src/commands/mod.rs` | Modify — add `images` module |
| `src-tauri/src/commands/ollama.rs` | Modify — extend hint prompt + response |
| `src-tauri/src/config.rs` | Modify — `image_source`, `gemini_api_key` |
| `src-tauri/src/lib.rs` | Modify — register new command |
| `src/hooks/usePredictiveHint.ts` | Modify — expose `imagePrompt` |
| `src/hooks/useAiImages.ts` | Create — AI image generation hook |
| `src/hooks/useMiisImages.ts` | Unchanged |
| `src/components/ImageSuggestions.tsx` | Modify — support base64 images |
| `src/components/SettingsDrawer.tsx` | Modify — image source selector |
| `src/components/modes/RecordingMode.tsx` | Modify — wire `imageSource` |
| `src/components/modes/ContinuousMode.tsx` | Modify — wire `imageSource` |
| `src/App.tsx` | Modify — pass `imageSource` |
| `src/types/index.ts` | Modify — add `image_source`, `gemini_api_key` |
