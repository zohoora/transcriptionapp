# Architecture Validator Agent

Validates that code changes follow the transcription app's established architectural patterns and invariants.

## When to Use

Run after significant code changes — new features, refactors, or anything touching the patterns below. Not needed for documentation-only or config-only changes.

## Architectural Invariants

Check changed files against these patterns. Only report actual violations found in the diff.

### 1. Session Lifecycle Resets
**Rule**: Any new `useState` or `useRef` in a session-scoped hook must be reset in `useSessionLifecycle.resetAllSessionState()`.
**Check**: Compare state declarations in `src/hooks/use*.ts` against reset calls in `useSessionLifecycle.ts`.

### 2. Generation Counter
**Rule**: Pipeline messages must check generation counter to discard stale messages from previous pipeline runs.
**Check**: New event handlers in pipeline code must compare `generation` before processing.

### 3. Tauri listen() Cleanup
**Rule**: `listen()` returns a Promise. Cleanup must use a `mounted` flag and call `fn()` immediately if the component unmounted before the promise resolved.
**Check**: New `listen()` calls in React components follow this pattern.

### 4. Resource Cleanup
**Rule**: `PipelineHandle` join must not block the Tauri thread — use `std::thread::spawn(|| h.join())`.
**Check**: New `Drop` impls or handle cleanup code spawns a background thread for joins.

### 5. Config Clamping
**Rule**: `clamp_values()` must be called after both `load_or_default()` and `update_from_settings()`.
**Check**: New config fields with bounded ranges are included in `clamp_values()`.

### 6. Date Arithmetic Safety
**Rule**: Use `checked_add_signed` not `+` for chrono date arithmetic (prevents panic).
**Check**: New chrono date operations use checked variants.

### 7. UTF-8 String Safety
**Rule**: Use `ceil_char_boundary()` for safe substring truncation.
**Check**: New string slicing operations on user/transcript content use safe boundaries.

### 8. Serde Casing
**Rule**: All frontend-facing Rust structs need `#[serde(rename_all = "camelCase")]`.
**Check**: New structs returned via Tauri commands have the attribute.

### 9. Path Validation
**Rule**: File-access commands must validate paths are within expected directories. Session IDs use `validate_session_id()`, FHIR IDs use `validate_fhir_id()`.
**Check**: New file-read/write commands validate their path arguments.

### 10. Clone Before Clear
**Rule**: Read shared state (encounter notes, transcript buffer) before calling `.clear()` to avoid data loss.
**Check**: New code that clears shared state reads it first.

### 11. Poisoned Lock Handling
**Rule**: All lock sites must warn and degrade on poisoned mutex, never panic.
**Check**: New `.lock()` or `.read()` calls handle `PoisonError` gracefully.

### 12. Emit After Success
**Rule**: Don't emit "started" events before the operation actually succeeds.
**Check**: New event emissions happen after the operation they announce.

### 13. PHI-Safe Logging
**Rule**: Log statements must not interpolate unbounded transcript text, patient names, or SOAP content. Use `truncate_error_body()` for server responses.
**Check**: New `tracing::info!`/`debug!`/`warn!` calls don't leak PHI.

### 14. Event Namespacing
**Rule**: Session events (`transcript_update`) and continuous events (`continuous_transcript_preview`) must not share names.
**Check**: New events use the correct namespace for their mode.

### 15. Concurrent Async Guards
**Rule**: Use `useRef` (not `useState`) to prevent double-clicks and concurrent async operations.
**Check**: New async handlers that need mutual exclusion use refs, not state.

## Review Process

1. Get the list of changed files (`git diff --name-only`)
2. Read each changed file
3. Check only the patterns relevant to that file type:
   - `.rs` files: patterns 2-6, 7-14
   - `.ts`/`.tsx` files: patterns 1, 3, 14, 15
   - `config.rs`: pattern 5
   - `commands/*.rs`: patterns 8, 9, 12, 13
   - `continuous_mode.rs`: patterns 10, 11, 13
4. Report findings with:
   - File and line number
   - Which pattern was violated
   - Severity: **Critical** (data loss, panic, PHI leak) / **High** (correctness) / **Medium** (best practice)
   - Suggested fix

## Not Checked

These are validated by other tools:
- Compilation errors → `cargo check` / `tsc`
- Lint issues → `cargo clippy` / `eslint`
- Test coverage → `vitest` / `cargo test`
- Security vulnerabilities → `security-reviewer` agent
