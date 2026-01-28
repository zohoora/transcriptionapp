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

### Authentication & OAuth
- `tauri-app/src-tauri/src/medplum.rs` - Medplum FHIR OAuth client
- `tauri-app/src-tauri/src/llm_client.rs` - LLM API authentication
- Check for token exposure in logs or errors
- Verify secure token storage

### Audio Data
- `tauri-app/src-tauri/src/audio.rs` - Audio capture
- `tauri-app/src-tauri/src/session.rs` - Recording state
- Ensure audio buffers are properly cleared after use
- Verify no audio data persists unintentionally

### IPC Command Validation
- `tauri-app/src-tauri/src/commands/` - Tauri command handlers
- Validate all inputs from frontend
- Check for command injection risks

### Environment Variables
- Verify sensitive values aren't hardcoded
- Check .gitignore includes env files

## Security Checklist

- [ ] No PHI in log output (check `log::`, `println!`, `eprintln!`)
- [ ] OAuth tokens not exposed in errors or debug output
- [ ] Audio buffers zeroed/dropped after use
- [ ] IPC inputs validated before processing
- [ ] No hardcoded secrets or API keys
- [ ] Sensitive files in .gitignore
- [ ] Debug-only code gated by `#[cfg(debug_assertions)]`
- [ ] Error messages don't leak sensitive details

## Review Process

1. Identify changed files in security-sensitive areas
2. Check each change against the checklist
3. Look for patterns that could leak PHI
4. Verify error handling doesn't expose secrets
5. Report findings with severity (Critical/High/Medium/Low)
