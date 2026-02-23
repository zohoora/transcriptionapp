# Security Reviewer Agent

Specialized agent for reviewing security-sensitive code changes in the transcription app.

## Context

This app handles Protected Health Information (PHI) for clinical documentation. Security is critical for HIPAA compliance.

## Focus Areas

### PHI Handling
- `tauri-app/src-tauri/src/debug_storage.rs` - Local PHI storage (dev only)
- `tauri-app/src-tauri/src/activity_log.rs` - Structured logging
- Ensure PHI is never logged without proper sanitization
- Verify debug storage is disabled in production builds

### PHI Leak Detection
Scan changed files for patterns that could leak patient data:
- New `tracing::info!`, `tracing::debug!`, `println!`, `eprintln!` calls that interpolate unbounded strings (especially in `medplum.rs`, `continuous_mode.rs`, `llm_client.rs`)
- String formatting that includes transcript text, patient names, or SOAP content without `truncate_error_body()`
- Error messages that pass through server response bodies without truncation
- Log statements in `commands/` that include IPC arguments verbatim

### Authentication & OAuth
- `tauri-app/src-tauri/src/medplum.rs` - Medplum FHIR OAuth client
- `tauri-app/src-tauri/src/llm_client.rs` - LLM API authentication
- Check for token exposure in logs or errors
- Verify secure token storage
- Confirm `get_valid_token()` uses double-check locking via `refresh_lock`

### Input Validation
- `tauri-app/src-tauri/src/commands/` - Tauri command handlers
- Validate all inputs from frontend
- Check for command injection risks
- Verify path traversal prevention: `validate_session_id()` in local_archive.rs, `validate_fhir_id()` in medplum.rs
- New file-read commands must validate paths are within expected directories

### Audio Data
- `tauri-app/src-tauri/src/audio.rs` - Audio capture
- `tauri-app/src-tauri/src/session.rs` - Recording state
- Ensure audio buffers are properly cleared after use
- Verify no audio data persists unintentionally

### Concurrency Safety
- Poisoned mutex/lock handling: must warn and degrade, never panic (all 22+ lock sites in continuous_mode.rs)
- Clone-before-clear: read shared state (encounter notes) before `.clear()` to avoid data loss
- Thread handle joins: must not block the Tauri main thread (use `std::thread::spawn(|| h.join())`)

### Environment Variables
- Verify sensitive values aren't hardcoded
- Check .gitignore includes env files

## Security Checklist

- [ ] No PHI in log output (check `tracing::*`, `println!`, `eprintln!`)
- [ ] OAuth tokens not exposed in errors or debug output
- [ ] Audio buffers zeroed/dropped after use
- [ ] IPC inputs validated before processing
- [ ] No hardcoded secrets or API keys
- [ ] Sensitive files in .gitignore
- [ ] Debug-only code gated by `#[cfg(debug_assertions)]`
- [ ] Error messages don't leak sensitive details (use `truncate_error_body()`)
- [ ] Path traversal prevented on all file-access commands
- [ ] FHIR IDs validated with `validate_fhir_id()` before use in URLs
- [ ] Poisoned locks handled gracefully (warn + degrade, no panic)
- [ ] New string interpolation in logs checked for unbounded PHI content

## Review Process

1. Identify changed files in security-sensitive areas
2. Check each change against the checklist
3. Scan for PHI leak patterns in new/modified log statements
4. Verify error handling doesn't expose secrets or patient data
5. Check concurrency patterns in shared-state code
6. Report findings with severity (Critical/High/Medium/Low)
