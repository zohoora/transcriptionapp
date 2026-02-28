# AI-Generated Medical Images Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace/supplement MIIS library images with AI-generated medical illustrations from Google Nano Banana 2 (Gemini), gated by the existing predictive hint LLM call.

**Architecture:** Extend the existing `generate_predictive_hint` LLM call to optionally produce an `image_prompt`. When the LLM says an image would help, the frontend calls a new `generate_ai_image` Tauri command that hits the Gemini API. A config toggle lets users choose between MIIS library, AI generation, or off.

**Tech Stack:** Rust (reqwest for Gemini HTTP calls), React/TypeScript (new `useAiImages` hook), Google Gemini API (`gemini-3.1-flash-image-preview`)

**Design doc:** `docs/plans/2026-02-28-ai-image-generation-design.md`

---

### Task 1: Add Config Fields

**Files:**
- Modify: `tauri-app/src-tauri/src/config.rs` (Settings struct ~line 133, defaults, clamp_values)
- Modify: `tauri-app/src/types/index.ts` (~line 94)
- Modify: `tauri-app/src/hooks/useSettings.ts` (~lines 35, 114, 187, 243)
- Modify: `tauri-app/src/test/mocks.ts` (~line 99)

**Step 1: Add Rust config fields**

In `config.rs`, add after the `miis_server_url` field (~line 137):

```rust
// AI image generation settings
#[serde(default = "default_image_source")]
pub image_source: String,
#[serde(default)]
pub gemini_api_key: String,
```

Add default function:

```rust
fn default_image_source() -> String {
    "off".to_string()
}
```

In `clamp_values()`, add:

```rust
// image_source must be "off", "miis", or "ai"
if !["off", "miis", "ai"].contains(&self.image_source.as_str()) {
    self.image_source = "off".to_string();
}
```

In `load_or_default()`, after `config.clamp_values()`, add migration:

```rust
// Migrate miis_enabled → image_source for backward compatibility
if config.miis_enabled && config.image_source == "off" {
    config.image_source = "miis".to_string();
}
```

**Step 2: Add TypeScript types**

In `types/index.ts`, after `miis_server_url: string;` (~line 96), add:

```typescript
image_source: string; // "off" | "miis" | "ai"
gemini_api_key: string;
```

**Step 3: Update useSettings hook**

In `useSettings.ts`, add to PendingSettings interface (~line 37):

```typescript
image_source: string;
gemini_api_key: string;
```

Add to `mapSettings` function (~line 115):

```typescript
image_source: s.image_source,
gemini_api_key: s.gemini_api_key,
```

Add to `saveSettings` function (~line 188):

```typescript
image_source: pendingSettings.image_source,
gemini_api_key: pendingSettings.gemini_api_key,
```

Add to `hasUnsavedChanges` computed (~line 244):

```typescript
[settings.image_source, pendingSettings.image_source],
[settings.gemini_api_key, pendingSettings.gemini_api_key],
```

**Step 4: Update test mocks**

In `test/mocks.ts`, add to the mock settings object (~line 99):

```typescript
image_source: 'off',
gemini_api_key: '',
```

**Step 5: Verify**

Run: `cd tauri-app/src-tauri && cargo check`
Run: `cd tauri-app && npx tsc --noEmit`
Expected: Both pass

**Step 6: Commit**

```
feat: add image_source and gemini_api_key config fields
```

---

### Task 2: Gemini Client

**Files:**
- Create: `tauri-app/src-tauri/src/gemini_client.rs`

**Step 1: Write unit test**

At the bottom of the new file, add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_body() {
        let body = GeminiClient::build_request_body("Draw a knee", "4:3");
        let json: serde_json::Value = serde_json::from_str(&serde_json::to_string(&body).unwrap()).unwrap();
        assert_eq!(json["contents"][0]["parts"][0]["text"], "Draw a knee");
        assert_eq!(json["generationConfig"]["responseModalities"][0], "IMAGE");
        assert_eq!(json["generationConfig"]["imageConfig"]["aspectRatio"], "4:3");
    }

    #[test]
    fn test_parse_response_valid() {
        let response_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "inlineData": {
                            "mimeType": "image/png",
                            "data": "iVBORw0KGgo="
                        }
                    }]
                }
            }]
        });
        let response: GeminiResponse = serde_json::from_value(response_json).unwrap();
        let base64 = GeminiClient::extract_image_base64(&response);
        assert_eq!(base64, Some("iVBORw0KGgo=".to_string()));
    }

    #[test]
    fn test_parse_response_no_image() {
        let response_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "text": "I cannot generate that image"
                    }]
                }
            }]
        });
        let response: GeminiResponse = serde_json::from_value(response_json).unwrap();
        let base64 = GeminiClient::extract_image_base64(&response);
        assert!(base64.is_none());
    }

    #[test]
    fn test_parse_response_empty_candidates() {
        let response_json = serde_json::json!({
            "candidates": []
        });
        let response: GeminiResponse = serde_json::from_value(response_json).unwrap();
        let base64 = GeminiClient::extract_image_base64(&response);
        assert!(base64.is_none());
    }

    #[test]
    fn test_new_empty_api_key() {
        let result = GeminiClient::new("");
        assert!(result.is_err());
    }

    #[test]
    fn test_new_valid_api_key() {
        let result = GeminiClient::new("test-key-123");
        assert!(result.is_ok());
    }
}
```

**Step 2: Implement the client**

```rust
//! Google Gemini API client for AI image generation
//!
//! Thin wrapper around the Gemini generateContent endpoint for
//! generating medical illustrations via Nano Banana 2.

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

const GEMINI_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const DEFAULT_MODEL: &str = "gemini-3.1-flash-image-preview";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct GeminiClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

// -- Request types --

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    generation_config: GeminiGenerationConfig,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum GeminiPart {
    Text(String),
    InlineData(GeminiInlineData),
}

// Custom serialization for the request part (text only)
impl GeminiPart {
    fn text(s: &str) -> serde_json::Value {
        serde_json::json!({"text": s})
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiInlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    response_modalities: Vec<String>,
    image_config: GeminiImageConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiImageConfig {
    aspect_ratio: String,
}

// -- Response types --

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponsePart {
    inline_data: Option<GeminiInlineData>,
    text: Option<String>,
}

impl GeminiClient {
    pub fn new(api_key: &str) -> Result<Self, String> {
        if api_key.trim().is_empty() {
            return Err("Gemini API key is required".to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            client,
            api_key: api_key.to_string(),
            model: DEFAULT_MODEL.to_string(),
        })
    }

    pub fn build_request_body(prompt: &str, aspect_ratio: &str) -> serde_json::Value {
        serde_json::json!({
            "contents": [{
                "parts": [{"text": prompt}]
            }],
            "generationConfig": {
                "responseModalities": ["IMAGE"],
                "imageConfig": {
                    "aspectRatio": aspect_ratio
                }
            }
        })
    }

    pub fn extract_image_base64(response: &GeminiResponse) -> Option<String> {
        response.candidates.first()
            .and_then(|c| c.content.parts.iter().find_map(|p| p.inline_data.as_ref()))
            .map(|d| d.data.clone())
    }

    pub async fn generate_image(&self, prompt: &str) -> Result<String, String> {
        let url = format!("{}/{}:generateContent", GEMINI_ENDPOINT, self.model);
        let body = Self::build_request_body(prompt, "4:3");

        info!("Gemini image generation: prompt={} chars", prompt.len());

        let response = self.client
            .post(&url)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .header("x-goog-api-key", HeaderValue::from_str(&self.api_key)
                .map_err(|e| format!("Invalid API key header: {}", e))?)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini API request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            // Truncate error body to avoid leaking sensitive data
            let truncated = if error_body.len() > 200 { &error_body[..200] } else { &error_body };
            return Err(format!("Gemini API error {}: {}", status, truncated));
        }

        let gemini_response: GeminiResponse = response.json().await
            .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

        Self::extract_image_base64(&gemini_response)
            .ok_or_else(|| "Gemini response contained no image data".to_string())
    }
}
```

**Step 3: Add module to main lib**

In `tauri-app/src-tauri/src/lib.rs`, add `mod gemini_client;` alongside other module declarations.

**Step 4: Run tests**

Run: `cd tauri-app/src-tauri && cargo test gemini`
Expected: All 6 tests pass

**Step 5: Commit**

```
feat: add Gemini API client for AI image generation
```

---

### Task 3: Image Generation Command

**Files:**
- Create: `tauri-app/src-tauri/src/commands/images.rs`
- Modify: `tauri-app/src-tauri/src/commands/mod.rs` (~line 11, ~line 28)
- Modify: `tauri-app/src-tauri/src/lib.rs` (~line 380)

**Step 1: Create the command**

```rust
//! AI image generation command

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::config::Config;
use crate::gemini_client::GeminiClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiImageResponse {
    pub image_base64: String,
    pub prompt: String,
}

#[tauri::command]
pub async fn generate_ai_image(prompt: String) -> Result<AiImageResponse, String> {
    if prompt.trim().is_empty() {
        return Err("Image prompt is empty".to_string());
    }

    let config = Config::load_or_default();

    if config.image_source != "ai" {
        return Err("AI image generation is not enabled".to_string());
    }

    let client = GeminiClient::new(&config.gemini_api_key)?;

    info!("Generating AI image: prompt={} chars", prompt.len());

    let image_base64 = client.generate_image(&prompt).await?;

    info!("AI image generated: {} bytes base64", image_base64.len());

    Ok(AiImageResponse {
        image_base64,
        prompt,
    })
}
```

**Step 2: Register module and command**

In `commands/mod.rs`, add `mod images;` after `mod miis;` (~line 11) and `pub use images::*;` after `pub use miis::*;` (~line 28).

In `lib.rs`, add `commands::generate_ai_image,` after `commands::miis_send_usage,` (~line 381).

**Step 3: Verify**

Run: `cd tauri-app/src-tauri && cargo check`
Expected: Pass

**Step 4: Commit**

```
feat: add generate_ai_image Tauri command
```

---

### Task 4: Extend Predictive Hint Prompt

**Files:**
- Modify: `tauri-app/src-tauri/src/commands/ollama.rs` (~lines 293-400)

**Step 1: Extend response struct**

In `PredictiveHintResponse` (~line 295), add:

```rust
/// Optional image generation prompt (only when AI images are useful)
#[serde(default)]
pub image_prompt: Option<String>,
```

**Step 2: Extend system prompt**

Replace the system prompt string (~line 332) with the extended version that adds IMAGE_PROMPT as a third output field. The JSON format line becomes:

```
{"hint":"brief clinical fact here","concepts":[...],"image_prompt":"detailed medical illustration prompt or null"}
```

Add rules for image_prompt:
- Style: "medical illustration, anatomical diagram, labeled, clean white background"
- Be anatomically specific with view angle and relevant structures
- Do NOT generate for: lab values, medications, general wellness, psychological topics
- Maximum one image_prompt per response
- Return `null` for image_prompt when no image would help

**Step 3: Update parse_hint_response**

Find the `parse_hint_response` function and ensure it handles the optional `image_prompt` field from JSON. Since the field has `#[serde(default)]`, it will default to `None` if absent — existing responses still parse correctly.

**Step 4: Update empty_response**

In the `empty_response` construction (~line 315), add `image_prompt: None`.

**Step 5: Verify**

Run: `cd tauri-app/src-tauri && cargo check`
Run: `cd tauri-app/src-tauri && cargo test predictive`
Expected: Both pass

**Step 6: Commit**

```
feat: extend predictive hint to optionally produce image generation prompts
```

---

### Task 5: Frontend — usePredictiveHint Extension

**Files:**
- Modify: `tauri-app/src/hooks/usePredictiveHint.ts` (~lines 17-19, 27-30)

**Step 1: Extend response type**

In `PredictiveHintResponse` (~line 17), add:

```typescript
image_prompt: string | null;
```

**Step 2: Extend hook result**

In `UsePredictiveHintResult` (~line 27), add:

```typescript
/** Image generation prompt from LLM (null if no image needed) */
imagePrompt: string | null;
```

**Step 3: Add state and expose**

Add `const [imagePrompt, setImagePrompt] = useState<string | null>(null);` alongside existing state.

In the `generateHint` callback, after setting `setConcepts(parsed.concepts)`, add:

```typescript
setImagePrompt(parsed.image_prompt ?? null);
```

Add `imagePrompt` to the return object.

In the cleanup/reset logic (when recording stops), add `setImagePrompt(null)`.

**Step 4: Verify**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Pass

**Step 5: Commit**

```
feat: expose imagePrompt from usePredictiveHint hook
```

---

### Task 6: Frontend — useAiImages Hook

**Files:**
- Create: `tauri-app/src/hooks/useAiImages.ts`

**Step 1: Create the hook**

```typescript
import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

const COOLDOWN_MS = 45000; // 45 seconds between generations
const SESSION_CAP = 8; // Max images per session
const MAX_VISIBLE = 6; // Max images shown at once

export interface AiImage {
  base64: string;
  prompt: string;
  timestamp: number;
}

interface UseAiImagesOptions {
  imagePrompt: string | null;
  enabled: boolean; // image_source === "ai"
  sessionId: string | null;
}

interface UseAiImagesResult {
  images: AiImage[];
  isLoading: boolean;
  error: string | null;
  dismissImage: (index: number) => void;
}

export function useAiImages({
  imagePrompt,
  enabled,
  sessionId,
}: UseAiImagesOptions): UseAiImagesResult {
  const [images, setImages] = useState<AiImage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const lastGenerationTime = useRef(0);
  const lastPrompt = useRef<string | null>(null);
  const sessionCount = useRef(0);
  const isGenerating = useRef(false);

  // Reset on session change
  useEffect(() => {
    setImages([]);
    setError(null);
    lastGenerationTime.current = 0;
    lastPrompt.current = null;
    sessionCount.current = 0;
    isGenerating.current = false;
  }, [sessionId]);

  // Generate image when prompt changes
  useEffect(() => {
    if (!enabled || !imagePrompt || !sessionId) return;

    // Dedup: skip if same prompt
    if (imagePrompt === lastPrompt.current) return;

    // Session cap
    if (sessionCount.current >= SESSION_CAP) return;

    // Cooldown
    const now = Date.now();
    if (now - lastGenerationTime.current < COOLDOWN_MS) return;

    // Concurrency guard
    if (isGenerating.current) return;

    isGenerating.current = true;
    lastPrompt.current = imagePrompt;
    lastGenerationTime.current = now;
    setIsLoading(true);
    setError(null);

    invoke<{ image_base64: string; prompt: string }>('generate_ai_image', {
      prompt: imagePrompt,
    })
      .then((result) => {
        sessionCount.current += 1;
        setImages((prev) => {
          const next = [...prev, {
            base64: result.image_base64,
            prompt: result.prompt,
            timestamp: Date.now(),
          }];
          // FIFO cap
          return next.length > MAX_VISIBLE ? next.slice(next.length - MAX_VISIBLE) : next;
        });
      })
      .catch((e) => {
        setError(String(e));
      })
      .finally(() => {
        setIsLoading(false);
        isGenerating.current = false;
      });
  }, [imagePrompt, enabled, sessionId]);

  const dismissImage = useCallback((index: number) => {
    setImages((prev) => prev.filter((_, i) => i !== index));
  }, []);

  return { images, isLoading, error, dismissImage };
}
```

**Step 2: Verify**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Pass

**Step 3: Commit**

```
feat: add useAiImages hook with cooldown and session cap
```

---

### Task 7: Frontend — ImageSuggestions Update

**Files:**
- Modify: `tauri-app/src/components/ImageSuggestions.tsx`

**Step 1: Add AI image props**

Add new optional props alongside existing ones:

```typescript
// AI-generated image props
aiImages?: AiImage[];
aiLoading?: boolean;
aiError?: string | null;
onAiDismiss?: (index: number) => void;
imageSource?: 'miis' | 'ai' | 'off';
```

Import `AiImage` from `useAiImages`.

**Step 2: Add AI rendering path**

When `imageSource === "ai"`, render `aiImages` as base64 `<img>` tags using `data:image/png;base64,{base64}` src. Reuse the same thumbnail strip layout, click-to-expand behavior, and dismiss button. Skip telemetry calls (no MIIS server).

When `imageSource === "miis"` or not set, keep existing behavior unchanged.

**Step 3: Verify**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Pass

**Step 4: Commit**

```
feat: support AI-generated base64 images in ImageSuggestions
```

---

### Task 8: Frontend — Settings UI

**Files:**
- Modify: `tauri-app/src/components/SettingsDrawer.tsx` (~lines 490-522)

**Step 1: Replace MIIS toggle with image source selector**

Replace the `miis_enabled` checkbox and conditional URL input with:

- A 3-option `<select>` dropdown: Off / MIIS Library / AI Generated
- When "miis" selected: show MIIS server URL input (existing)
- When "ai" selected: show Gemini API key password input
- Map the selection to `pendingSettings.image_source`

**Step 2: Verify**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Pass

**Step 3: Commit**

```
feat: add image source selector to settings (off/miis/ai)
```

---

### Task 9: Frontend — Wire Into App and Mode Components

**Files:**
- Modify: `tauri-app/src/App.tsx` (~lines 320-330, 630-637, 687-694)
- Modify: `tauri-app/src/components/modes/RecordingMode.tsx` (~lines 49-57, 91-98, 192-202)
- Modify: `tauri-app/src/components/modes/ContinuousMode.tsx` (~lines 52, 139, 372)

**Step 1: Initialize useAiImages in App.tsx**

Import `useAiImages`. Initialize it alongside `useMiisImages`:

```typescript
const { images: aiImages, isLoading: aiLoading, error: aiError, dismissImage: aiDismiss } = useAiImages({
  imagePrompt,
  enabled: (settings?.image_source ?? 'off') === 'ai',
  sessionId: status.session_id ?? null,
});
```

Pass `imageSource`, `aiImages`, `aiLoading`, `aiError`, `onAiDismiss` to RecordingMode alongside existing MIIS props.

**Step 2: Update RecordingMode**

Add AI image props to the interface. Pass them to `ImageSuggestions`. Change the rendering condition from `miisEnabled` to `imageSource !== 'off'`:

```typescript
{imageSource !== 'off' && (
  <ImageSuggestions
    suggestions={miisSuggestions}
    /* ...existing MIIS props... */
    aiImages={aiImages}
    aiLoading={aiLoading}
    aiError={aiError}
    onAiDismiss={onAiDismiss}
    imageSource={imageSource}
  />
)}
```

**Step 3: Update ContinuousMode**

Same pattern — pass `imageSource` and AI image props. The continuous mode orchestrator will need the same `useAiImages` integration (check `useContinuousModeOrchestrator.ts`).

**Step 4: Verify**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Pass

**Step 5: Commit**

```
feat: wire AI image generation into recording and continuous modes
```

---

### Task 10: Test and Build

**Step 1: Run all Rust tests**

Run: `cd tauri-app/src-tauri && cargo test`
Expected: All pass (561+ tests)

**Step 2: Run all frontend tests**

Run: `cd tauri-app && pnpm test:run`
Expected: All pass (414 tests)

**Step 3: Run TypeScript check**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Clean

**Step 4: Build debug**

Run: `cd tauri-app && pnpm tauri build --debug`
Expected: Build succeeds

**Step 5: Commit any test fixes**

```
fix: resolve test failures from AI image integration
```

---

### Task 11: Update Documentation

**Files:**
- Modify: `tauri-app/CLAUDE.md`
- Modify: Root `CLAUDE.md` (test counts if changed)

**Step 1: Update CLAUDE.md**

- Add `generate_ai_image` to IPC Commands table under a new "Images" row
- Update Settings Schema to include `image_source`, `gemini_api_key`
- Update Features table: change MIIS description to mention AI generation option
- Add `gemini_client.rs` and `commands/images.rs` to Architecture section
- Add `useAiImages.ts` to Key Hooks table

**Step 2: Commit**

```
docs: update CLAUDE.md with AI image generation feature
```
