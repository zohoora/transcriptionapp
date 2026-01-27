# ADR 0016: Speaker-Verified Auto-Start

## Status
Accepted

## Context
The auto-session detection feature (ADR-0011) triggers recording when speech containing a greeting is detected. However, this can cause false starts when:
- Non-clinical staff pass by the recording station
- Patients in waiting areas are picked up by sensitive microphones
- Background conversations trigger detection

For clinics with enrolled speaker profiles (ADR-0014), we can leverage voice recognition to ensure only authorized users trigger auto-start.

## Decision
Add optional speaker verification to the auto-start listening flow.

### Configuration
```rust
auto_start_require_enrolled: bool,     // Require enrolled speaker for auto-start
auto_start_required_role: Option<String>,  // Optional role filter (e.g., "physician")
```

### Verification Flow
1. VAD detects sustained speech (2+ seconds)
2. **NEW**: If `require_enrolled` is enabled:
   - Extract voice embedding from speech buffer
   - Compare against all enrolled profiles using cosine similarity
   - If `required_role` is set, only check profiles with matching role
   - Threshold: 0.6 (same as enrolled speaker recognition)
3. If speaker verified (or verification disabled):
   - Proceed with optimistic recording + greeting check
4. If speaker NOT verified:
   - Emit `speaker_not_verified` event
   - Enter cooldown period
   - Continue listening for next speech

### Components
- **SpeakerVerifier** (`listening.rs`): Loads profiles, extracts embeddings, matches
- **ListeningConfig**: New fields for verification settings
- **ListeningEvent::SpeakerNotVerified**: New event when verification fails

### UI Changes
Settings drawer shows nested options under "Auto-start on Greeting":
- Toggle: "Require Enrolled Speaker"
- Dropdown: "Required Role" (only visible when toggle enabled)

## Consequences

### Positive
- Prevents false auto-starts from non-clinical personnel
- Role filtering enables physician-only recording policies
- Reuses existing speaker enrollment infrastructure
- No additional models or setup required

### Negative
- Requires at least one enrolled speaker profile
- Adds ~200-500ms latency for embedding extraction
- Speaker must be enrolled before feature is useful
- Voice changes (illness, time of day) may cause verification failures

### Mitigations
- Feature is opt-in (disabled by default)
- Clear error messages when no profiles enrolled
- Reasonable similarity threshold (0.6) balances security vs convenience
- Failed verification just continues listening (no permanent lockout)
