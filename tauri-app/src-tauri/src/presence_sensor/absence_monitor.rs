//! Absence threshold monitor.
//!
//! Watches the debounced presence state. When it transitions to Absent and stays
//! Absent for `threshold_secs`, fires the absence_trigger Notify.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, Notify};
use tracing::{debug, info};

use super::types::PresenceState;

pub async fn absence_monitor(
    mut state_rx: watch::Receiver<PresenceState>,
    absence_trigger: Arc<Notify>,
    stop: Arc<AtomicBool>,
    threshold_secs: u64,
) {
    let threshold = Duration::from_secs(threshold_secs);

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        // Wait for state to become Absent
        let became_absent = loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let current = *state_rx.borrow();
            if current == PresenceState::Absent {
                break true;
            }
            // Wait for a state change
            if state_rx.changed().await.is_err() {
                // Sender dropped
                return;
            }
        };

        if !became_absent {
            continue;
        }

        debug!(
            "Absence monitor: room became absent, starting {}s timer",
            threshold_secs
        );

        // Start the absence timer
        let timer_start = Instant::now();

        loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }

            let remaining = threshold.saturating_sub(timer_start.elapsed());
            if remaining.is_zero() {
                // Threshold exceeded — fire the trigger
                info!(
                    "Absence threshold reached ({}s) — triggering encounter split",
                    threshold_secs
                );
                absence_trigger.notify_one();
                break;
            }

            // Wait for either a state change or the remaining time to elapse
            tokio::select! {
                result = state_rx.changed() => {
                    if result.is_err() {
                        return; // Sender dropped
                    }
                    let current = *state_rx.borrow();
                    if current == PresenceState::Present {
                        debug!("Absence monitor: room became present, cancelling timer");
                        break; // Back to outer loop — wait for next absence
                    }
                    // Still absent (or unknown) — continue timing
                }
                _ = tokio::time::sleep(remaining) => {
                    // Timer expired — will trigger on next iteration
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_absence_threshold_fires_after_duration() {
        let (state_tx, _) = watch::channel(PresenceState::Present);
        let state_tx = Arc::new(state_tx);
        let trigger = Arc::new(Notify::new());
        let stop = Arc::new(AtomicBool::new(false));

        let state_rx = state_tx.subscribe();
        let trigger_clone = trigger.clone();
        let stop_clone = stop.clone();

        let monitor = tokio::spawn(async move {
            absence_monitor(state_rx, trigger_clone, stop_clone, 1).await;
        });

        // Transition to absent
        let _ = state_tx.send(PresenceState::Absent);

        // Should trigger within ~1.5 seconds
        let result = tokio::time::timeout(Duration::from_secs(3), trigger.notified()).await;
        assert!(result.is_ok(), "Absence trigger should fire after threshold");

        stop.store(true, Ordering::Relaxed);
        monitor.abort();
    }

    #[tokio::test]
    async fn test_absence_cancelled_by_return_to_present() {
        let (state_tx, _) = watch::channel(PresenceState::Present);
        let state_tx = Arc::new(state_tx);
        let trigger = Arc::new(Notify::new());
        let stop = Arc::new(AtomicBool::new(false));

        let state_rx = state_tx.subscribe();
        let trigger_clone = trigger.clone();
        let stop_clone = stop.clone();

        // 2s threshold
        let monitor = tokio::spawn(async move {
            absence_monitor(state_rx, trigger_clone, stop_clone, 2).await;
        });

        // Absent
        let _ = state_tx.send(PresenceState::Absent);
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Return to present before threshold
        let _ = state_tx.send(PresenceState::Present);

        // Trigger should NOT fire
        let result = tokio::time::timeout(Duration::from_secs(3), trigger.notified()).await;
        assert!(result.is_err(), "Trigger should NOT fire when person returns");

        stop.store(true, Ordering::Relaxed);
        monitor.abort();
    }
}
