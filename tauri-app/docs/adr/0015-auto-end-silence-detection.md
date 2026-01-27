# ADR 0015: Auto-End Silence Detection

## Status
Accepted

## Context
Recording sessions may continue indefinitely if the user forgets to stop them. This wastes storage, processing time, and creates unnecessarily long transcripts. In clinical settings, encounters naturally end with a period of silence as the patient leaves.

## Decision
Implement automatic session termination after prolonged silence with user-cancellable countdown.

### Configuration
```rust
auto_end_enabled: bool,        // Enable/disable feature
auto_end_silence_ms: u64,      // Silence threshold (default: 120000ms = 2 minutes)
```

### Detection Flow
1. VAD tracks continuous silence during active recording
2. When silence exceeds half the threshold (60s by default):
   - Emit `silence_warning` event with countdown
   - UI shows "Auto-ending in X:XX" with "Keep Recording" button
3. User can cancel via `reset_silence_timer` command
4. If not cancelled and silence continues:
   - Emit `session_auto_end` event
   - Trigger graceful session stop (same as manual stop)

### Events
```typescript
// Emitted during countdown (every second)
interface SilenceWarningPayload {
  silence_ms: number;     // Total silence duration so far
  remaining_ms: number;   // Time until auto-end (0 = cancelled)
}

// Emitted when session auto-ends
interface AutoEndEventPayload {
  reason: 'silence';
  silence_duration_ms: number;
}
```

### Implementation
- **Backend** (`pipeline.rs`): Track `continuous_silence_start: Option<Instant>`
- **Backend** (`commands/session.rs`): Handle silence events, `reset_silence_timer` command
- **Frontend** (`useSessionState.ts`): Listen for events, show countdown UI
- **Frontend** (`RecordingMode.tsx`): Display warning banner with cancel button

## Consequences

### Positive
- Prevents forgotten recordings from running indefinitely
- Graceful shutdown preserves all recorded audio and transcript
- User retains full control via cancel mechanism
- Warning countdown prevents accidental data loss

### Negative
- Brief pauses (thinking, reviewing notes) could trigger false warnings
- Requires user awareness of the countdown UI
- 2-minute default may be too aggressive for some workflows

### Mitigations
- Warning at 50% threshold gives ample time to cancel
- Clear visual/audio feedback during countdown
- Configurable threshold allows per-workflow tuning
- Feature can be disabled entirely if not desired
