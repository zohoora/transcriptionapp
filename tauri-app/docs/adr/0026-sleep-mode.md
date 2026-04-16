# ADR-0026: Sleep Mode (Overnight Auto-Pause)

## Status

Accepted (Apr 2026)

## Context

Continuous mode is designed to run for a full clinic day, but some rooms leave the app running overnight:

- Admin forgets to stop it at close.
- App launches automatically at login (Mac auto-login for clinic workstations).
- Sensor occasionally triggers at night from cleaning staff.

Consequences:

- **STT + LLM cost** on empty air for 8+ hours per room.
- **Spurious encounter splits** from night-time sensor noise.
- **Confusing history** — a "session" at 3 AM with 4 words in it clutters the next morning's review.

## Decision

Auto-pause continuous mode between configurable hours (default 22:00–06:00 EST). At the start of the sleep window, stop the active continuous-mode handle cleanly (same path as a user-initiated stop); at the wake hour, auto-restart.

### Config

| Field | Default | Clamp | Purpose |
|-------|---------|-------|---------|
| `sleep_mode_enabled` | `true` | — | Master switch |
| `sleep_start_hour` | `22` | 0–23 | Hour to stop (local EST/EDT) |
| `sleep_end_hour` | `6` | 0–23 | Hour to resume |

### Implementation

The outer loop in `commands/continuous.rs` wraps `run_continuous_mode`. At every iteration:

1. Compute `now_eastern = Utc::now().with_timezone(&chrono_tz::America::New_York)`.
2. If `sleep_mode_enabled && is_in_sleep_window(now_eastern, start, end)`:
   - Set the inner handle's `stop_flag` → normal cleanup (flushes current buffer, archives any pending encounter, uploads aux files).
   - Compute the wake deadline and `tokio::time::sleep` until then (or until cancelled).
   - Auto-restart at the wake hour by looping back to `run_continuous_mode`.
3. Otherwise: run normally.

### DST handling

`chrono_tz::America::New_York` correctly handles both EST and EDT transitions. The wake deadline is computed in local time, then converted to UTC for the sleep duration — so "wake at 06:00 local" works correctly even across spring-forward and fall-back boundaries.

### UI

- **Sleep banner** in ContinuousMode.tsx visible during the sleep window ("Sleeping until 6:00 AM — will auto-resume").
- Events: `sleep_started` (with wake time), `sleep_ended` (on auto-resume).
- User can manually resume early by clicking "Wake now" which cancels the outer sleep future.

### Day log rotation

`DayLogger` checks the local date on each `log()` call and opens a new file if the date rolled over. Sleep mode exits mid-night relative to UTC but within the same local day, so this is a no-op on most nights; on DST days it correctly rotates at local midnight.

## Consequences

### Positive

- **Zero LLM/STT cost overnight** — the app is effectively off even though the process is running.
- **Night-time sensor noise is ignored** — presence observer doesn't need special-case handling because continuous mode is fully stopped.
- **DST-safe** — `chrono-tz` eliminates the class of bugs where "wake at 6 AM" drifts an hour twice a year.
- **No config change after deploy** — defaults match typical clinic hours.

### Negative

- **Hardcoded to US Eastern** — `chrono_tz::America::New_York` is baked in. If the app ever deploys outside Ontario, the timezone needs to become configurable.
- **No grace period for late appointments** — if a physician is finishing a note at 10 PM and hasn't stopped continuous mode, sleep mode will stop the capture. The physician can disable sleep mode for that day from settings, but it's a surprise the first time.
- **"Wake now" requires the app to be open** — if the physician shows up early and the app window is collapsed, they may not see the sleep banner. Minor UX issue.

## References

- `tauri-app/src-tauri/src/commands/continuous.rs` — outer loop with sleep wrapping
- `tauri-app/src-tauri/Cargo.toml` — `chrono-tz` dependency
- `tauri-app/src/components/modes/ContinuousMode.tsx` — sleep banner
