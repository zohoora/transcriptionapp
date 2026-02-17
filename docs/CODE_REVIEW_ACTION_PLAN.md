# Code Review Action Plan

> **Historical Document (2026-01-15)**: This action plan was based on the detailed review in `DETAILED_CODE_REVIEW.md`. Several items have been resolved. For the current status of all review findings, see [CODE_REVIEW_FINDINGS.md](../CODE_REVIEW_FINDINGS.md) (updated 2026-02-17). Items marked below with ~~strikethrough~~ have been addressed.

Based on the detailed code review in `DETAILED_CODE_REVIEW.md`, this document outlines the validated findings and prioritized remediation plan.

## Validation Summary

All P0 findings have been verified against the actual codebase:

| Finding | Status (Jan 2026) | Status (Feb 2026) | Location |
|---------|-------------------|-------------------|----------|
| PHI in logs (transcript) | **Confirmed** | Still open | `listening.rs` |
| PHI in logs (patient search) | **Confirmed** | Still open | `commands/medplum.rs` |
| OAuth code in logs | **Confirmed** | Still open | `lib.rs` |
| OAuth code in console | **Confirmed** | Still open | `AuthProvider.tsx` |
| Debug storage default=true | **Confirmed** | **FIXED** (`cfg!(debug_assertions)`) | `config.rs` |
| Hard-coded internal IPs | **Confirmed** | Partially fixed (Medplum default now empty) | `config.rs` |
| Default API key shipped | **Confirmed** | **FIXED** (now empty string) | `config.rs` |
| Hard-coded "fast-model" | **Confirmed** | **FIXED** (uses `self.fast_model`) | `llm_client.rs` |
| Secrets stored unencrypted | **Confirmed** | Still open | `medplum.rs`, `config.rs` |

---

## Phase 0: Critical Security Fixes (P0)

**Target: 1-2 days** | Status: 0.2 and 0.3 completed (Feb 2026), 0.1 still open

### 0.1 Remove PHI from Logs (STILL OPEN)

**Files to modify:**

1. **`src-tauri/src/listening.rs`**
   - Line 543: Change `info!("Not a greeting, rejecting: '{}'", result.transcript)` to log only length/word count
   - Line 635: Change `info!("Transcript: '{}'", transcript)` to log only metadata

   ```rust
   // Before
   info!("Not a greeting, rejecting: '{}'", result.transcript);
   info!("Transcript: '{}'", transcript);

   // After
   info!("Not a greeting, rejecting (len={})", result.transcript.len());
   info!("Transcribed {} chars, {} words", transcript.len(), transcript.split_whitespace().count());
   ```

2. **`src-tauri/src/commands/medplum.rs`**
   - Line 191: Change `info!("Searching for patients: {}", query)` to not log the query

   ```rust
   // Before
   info!("Searching for patients: {}", query);

   // After
   info!("Searching for patients (query_len={})", query.len());
   ```

3. **`src-tauri/src/lib.rs`**
   - Line 176: Change `info!("Deep link received via single instance: {}", arg)` to strip sensitive params

   ```rust
   // Before
   info!("Deep link received via single instance: {}", arg);

   // After
   let safe_url = arg.split('?').next().unwrap_or("fabricscribe://");
   info!("Deep link received: {} (has_params={})", safe_url, arg.contains('?'));
   ```

4. **`src/components/AuthProvider.tsx`**
   - Lines 63, 73, 87, 95: Remove or sanitize console.log statements

   ```typescript
   // Before
   console.log('Processing deep link:', url);
   console.log('Deep link at startup:', urls);
   console.log('Deep link received via plugin:', urls);
   console.log('Deep link received via event:', event.payload);

   // After
   console.log('Processing deep link (path only):', url.split('?')[0]);
   // Or remove entirely in production builds
   ```

### 0.2 Disable Debug Storage by Default -- FIXED (Feb 2026)

**File:** `src-tauri/src/config.rs` -- Now uses `cfg!(debug_assertions)`, only enabled in debug builds.

### 0.3 Remove Hard-coded Defaults -- PARTIALLY FIXED (Feb 2026)

**File:** `src-tauri/src/config.rs`

- ~~`llm_api_key`~~: Now defaults to empty string
- ~~`medplum_server_url`~~: Now defaults to empty string
- `whisper_server_url`: Still defaults to a network IP (`10.241.15.154:8001`) -- the configured STT Router

---

## Phase 1: Correctness Fixes (P1)

**Target: 3-5 days** | Status: 1.1 completed (Feb 2026), 1.2 and 1.3 still open

### 1.1 Fix Hard-coded "fast-model" in Greeting Detection -- FIXED (Feb 2026)

**File:** `src-tauri/src/llm_client.rs`

```rust
// Before (line 725)
let request = ChatCompletionRequest {
    model: "fast-model".to_string(),
    ...
};

// After - Add fast_model parameter to LLMClient
pub struct LLMClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    client_id: String,
    fast_model: String,  // Add this field
}

impl LLMClient {
    pub fn new(base_url: &str, api_key: &str, client_id: &str, fast_model: &str) -> Result<Self, String> {
        // ... existing code ...
        Ok(Self {
            client,
            base_url: cleaned_url.to_string(),
            api_key: api_key.to_string(),
            client_id: client_id.to_string(),
            fast_model: fast_model.to_string(),
        })
    }

    pub async fn check_greeting(...) -> Result<GreetingResult, String> {
        // Use self.fast_model instead of hard-coded string
        let request = ChatCompletionRequest {
            model: self.fast_model.clone(),
            ...
        };
    }
}
```

**Update call sites:**
- `commands/ollama.rs` - pass fast_model from config
- `commands/listening.rs` - pass fast_model when creating LLMClient

### 1.2 Standardize HTTP Error Handling in Medplum

**File:** `src-tauri/src/medplum.rs`

Add `error_for_status()` to `exchange_code` and `refresh_token` methods:

```rust
// Before
let response = self.client
    .post(&token_url)
    .form(&params)
    .send()
    .await?
    .json::<TokenResponse>()
    .await?;

// After
let response = self.client
    .post(&token_url)
    .form(&params)
    .send()
    .await?
    .error_for_status()
    .map_err(|e| MedplumError::Auth(format!("Token exchange failed: {}", e.status().unwrap_or_default())))?
    .json::<TokenResponse>()
    .await?;
```

### 1.3 Propagate Transcription Errors to UI

**File:** `src-tauri/src/pipeline.rs`

Add error tracking and emit session error after consecutive failures:

```rust
// Add to pipeline state
let mut consecutive_errors = 0;
const MAX_CONSECUTIVE_ERRORS: u32 = 3;

// In transcription error handling
Err(e) => {
    error!("Transcription failed: {}", e);
    consecutive_errors += 1;

    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
        let _ = tx.send(PipelineMessage::Error(format!(
            "Transcription service unavailable after {} attempts: {}",
            consecutive_errors, e
        )));
    }
}

// On success, reset counter
consecutive_errors = 0;
```

---

## Phase 2: Security Hardening (P1)

**Target: 1-2 weeks**

### 2.1 Move Secrets to Secure Storage

**Approach:** Use `tauri-plugin-stronghold` or OS keychain

**Files to modify:**
- `src-tauri/Cargo.toml` - add stronghold dependency
- `src-tauri/src/medplum.rs` - store tokens in stronghold instead of JSON file
- `src-tauri/src/config.rs` - store `llm_api_key` in stronghold

**Implementation outline:**

```rust
// New module: src-tauri/src/secure_storage.rs
use tauri_plugin_stronghold::stronghold::Stronghold;

pub struct SecureStorage {
    stronghold: Stronghold,
}

impl SecureStorage {
    pub fn store_secret(&self, key: &str, value: &str) -> Result<(), String>;
    pub fn get_secret(&self, key: &str) -> Result<Option<String>, String>;
    pub fn delete_secret(&self, key: &str) -> Result<(), String>;
}
```

### 2.2 Atomic Writes with Strict Permissions

**File:** `src-tauri/src/config.rs`

```rust
use std::os::unix::fs::PermissionsExt;
use tempfile::NamedTempFile;

pub fn save(&self) -> Result<(), ConfigError> {
    let json = serde_json::to_string_pretty(self)?;
    let config_path = Self::config_path();

    // Atomic write: temp file -> fsync -> rename
    let dir = config_path.parent().unwrap();
    let mut temp = NamedTempFile::new_in(dir)?;
    temp.write_all(json.as_bytes())?;
    temp.as_file().sync_all()?;

    // Set strict permissions before rename (Unix only)
    #[cfg(unix)]
    {
        let mut perms = temp.as_file().metadata()?.permissions();
        perms.set_mode(0o600);
        temp.as_file().set_permissions(perms)?;
    }

    temp.persist(&config_path)?;
    Ok(())
}
```

### 2.3 Model Download Integrity Verification

**File:** `src-tauri/src/models.rs`

```rust
use sha2::{Sha256, Digest};

// Add known hashes for models
const MODEL_HASHES: &[(&str, &str)] = &[
    ("ggml-large-v3-turbo.bin", "abc123..."),
    ("speaker_embedding.onnx", "def456..."),
    // ...
];

async fn verify_model_integrity(path: &Path, expected_hash: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open model: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if bytes_read == 0 { break; }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = format!("{:x}", hasher.finalize());
    if hash != expected_hash {
        return Err(format!("Model integrity check failed: expected {}, got {}", expected_hash, hash));
    }
    Ok(())
}
```

---

## Phase 3: Reliability Improvements (P2)

**Target: 2-4 weeks**

### 3.1 Reuse Tokio Runtime

**Files:** `src-tauri/src/whisper_server.rs`, `src-tauri/src/listening.rs`

Instead of creating a new runtime per call, either:
1. Pass a runtime handle from the caller
2. Use `reqwest::blocking` in synchronous contexts
3. Create one thread-local runtime and reuse it

### 3.2 Proper Thread Lifecycle Management

**Files:** `src-tauri/src/commands/session.rs`, `src-tauri/src/commands/listening.rs`

```rust
// Store JoinHandle and join on stop
pub struct PipelineState {
    running: bool,
    handle: Option<JoinHandle<()>>,
}

pub fn stop_session(...) {
    if let Some(handle) = state.handle.take() {
        // Join in background to avoid blocking
        std::thread::spawn(move || {
            let _ = handle.join();
        });
    }
}
```

### 3.3 Remove Forced Process Exit

**File:** `src-tauri/src/lib.rs`

Investigate and fix the ONNX runtime crash-on-drop issue rather than using `_exit(0)`.

---

## Phase 4: Code Quality (P2)

**Target: Ongoing**

### 4.1 Consolidate ErrorBoundary

Remove `src/ErrorBoundary.tsx` and use only `src/components/ErrorBoundary.tsx`.

### 4.2 Fix CI continue-on-error

Remove `continue-on-error: true` from critical CI jobs or split into required vs informational.

### 4.3 Clean Up Dead Code

- Remove local Whisper code paths if remote-only is the decision
- Or fully implement the `whisper_mode` switch if hybrid is needed

### 4.4 Add PHI Logging Regression Test

```rust
#[test]
fn test_no_phi_in_log_messages() {
    // Capture log output
    // Run greeting detection with known transcript
    // Assert log messages don't contain the transcript text
    // Assert log messages don't contain "code=", "state=", "Bearer "
}
```

---

## Implementation Order

| Priority | Item | Effort | Risk if Skipped | Status (Feb 2026) |
|----------|------|--------|-----------------|-------------------|
| **0.1** | Remove PHI from logs | 2h | HIPAA violation | Open |
| **0.2** | Disable debug storage default | 30m | PHI exposure | **FIXED** |
| **0.3** | Remove hard-coded defaults | 1h | Credential leak | Partially fixed |
| **1.1** | Fix hard-coded fast-model | 2h | Config ignored | **FIXED** |
| **1.2** | HTTP error handling | 1h | Silent failures | Open |
| **1.3** | Surface transcription errors | 2h | User confusion | Open |
| **2.1** | Secure storage | 1-2d | Token theft | Open |
| **2.2** | Atomic writes | 2h | Data corruption | Open |
| **2.3** | Model integrity | 4h | Supply chain risk | Open |
| **3.x** | Reliability | 1w | Performance/stability | Partially fixed |
| **4.x** | Code quality | Ongoing | Tech debt | Partially fixed |

---

## Notes

- Phase 0 items are blocking for any clinical deployment
- Phase 1 items should be completed before wider testing
- Phase 2+ can be done iteratively based on priority
- The review correctly identified that local Whisper is currently dead code - a decision should be made whether to remove it or complete the implementation
- As of Feb 2026, the codebase has 421 Rust unit tests (0 failures), 387 frontend tests, and 0 cargo/tsc warnings
- For the full resolution status of all code review findings, see [CODE_REVIEW_FINDINGS.md](../CODE_REVIEW_FINDINGS.md)
