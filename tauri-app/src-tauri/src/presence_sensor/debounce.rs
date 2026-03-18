//! Debounce FSM for presence sensor readings.
//!
//! Filters out rapid toggles (e.g., mmWave false positives from wall penetration)
//! by requiring a sustained state change for `debounce_dur` before transitioning.

use std::time::{Duration, Instant};

/// Debounce finite state machine.
///
/// Tracks a candidate state change and only commits it after the candidate
/// has been held for at least `debounce_dur`. This prevents rapid
/// oscillations (common with mmWave sensors) from triggering false transitions.
pub struct DebounceFsm {
    debounced: Option<bool>,
    candidate: Option<bool>,
    candidate_since: Instant,
    debounce_dur: Duration,
}

impl DebounceFsm {
    pub fn new(debounce_secs: u64) -> Self {
        Self {
            debounced: None,
            candidate: None,
            candidate_since: Instant::now(),
            debounce_dur: Duration::from_secs(debounce_secs),
        }
    }

    /// Process a raw reading and return the new debounced state if it changed.
    ///
    /// Returns `Some(true/false)` when the debounced state transitions.
    /// Returns `None` when the state hasn't changed (reading absorbed by debounce).
    pub fn process(&mut self, raw: bool, now: Instant) -> Option<bool> {
        match self.debounced {
            None => {
                // First reading — initialize
                self.debounced = Some(raw);
                self.candidate = Some(raw);
                self.candidate_since = now;
                Some(raw)
            }
            Some(current) => {
                if raw != current {
                    // Different from debounced state
                    if self.candidate == Some(raw) {
                        // Same as candidate — check if held long enough
                        if now.duration_since(self.candidate_since) >= self.debounce_dur {
                            self.debounced = Some(raw);
                            return Some(raw);
                        }
                    } else {
                        // New candidate
                        self.candidate = Some(raw);
                        self.candidate_since = now;
                    }
                } else {
                    // Same as debounced — reset candidate
                    self.candidate = Some(raw);
                    self.candidate_since = now;
                }
                None // No change
            }
        }
    }

    /// Get the current debounced state (None if no readings processed yet)
    pub fn current(&self) -> Option<bool> {
        self.debounced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_reading_initializes() {
        let mut fsm = DebounceFsm::new(10);
        let start = Instant::now();
        assert_eq!(fsm.process(true, start), Some(true));
        assert_eq!(fsm.current(), Some(true));
    }

    #[test]
    fn test_rapid_toggles_ignored() {
        let mut fsm = DebounceFsm::new(10);
        let start = Instant::now();

        assert_eq!(fsm.process(true, start), Some(true));

        // Rapid toggles within debounce window — should NOT change state
        assert_eq!(fsm.process(false, start + Duration::from_secs(1)), None);
        assert_eq!(fsm.process(true, start + Duration::from_secs(2)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(3)), None);
        assert_eq!(fsm.process(true, start + Duration::from_secs(4)), None);

        assert_eq!(fsm.debounced, Some(true));
    }

    #[test]
    fn test_sustained_change_transitions() {
        let mut fsm = DebounceFsm::new(10);
        let start = Instant::now();

        assert_eq!(fsm.process(true, start), Some(true));

        // Sustained absent for >10 seconds
        assert_eq!(fsm.process(false, start + Duration::from_secs(1)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(5)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(9)), None);
        // At 11 seconds, should transition
        assert_eq!(
            fsm.process(false, start + Duration::from_secs(11)),
            Some(false)
        );

        assert_eq!(fsm.debounced, Some(false));
    }

    #[test]
    fn test_reset_on_blip() {
        let mut fsm = DebounceFsm::new(10);
        let start = Instant::now();

        assert_eq!(fsm.process(true, start), Some(true));

        // Start going absent
        assert_eq!(fsm.process(false, start + Duration::from_secs(1)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(5)), None);

        // Blip back to present — resets candidate
        assert_eq!(fsm.process(true, start + Duration::from_secs(6)), None);

        // Now absent again — candidate resets, needs another 10s
        assert_eq!(fsm.process(false, start + Duration::from_secs(7)), None);
        assert_eq!(fsm.process(false, start + Duration::from_secs(15)), None);

        // 10s after the new candidate started at t=7 → transition at t=17
        assert_eq!(
            fsm.process(false, start + Duration::from_secs(17)),
            Some(false)
        );
    }

    #[test]
    fn test_absent_to_present_transition() {
        let mut fsm = DebounceFsm::new(5);
        let start = Instant::now();

        // Start absent
        assert_eq!(fsm.process(false, start), Some(false));

        // Sustained present
        assert_eq!(fsm.process(true, start + Duration::from_secs(1)), None);
        assert_eq!(
            fsm.process(true, start + Duration::from_secs(6)),
            Some(true)
        );
    }

    #[test]
    fn test_zero_debounce() {
        let mut fsm = DebounceFsm::new(0);
        let start = Instant::now();

        assert_eq!(fsm.process(true, start), Some(true));
        // First false sets the candidate
        assert_eq!(fsm.process(false, start + Duration::from_millis(1)), None);
        // Second false confirms the candidate (0s debounce means immediate on next reading)
        assert_eq!(
            fsm.process(false, start + Duration::from_millis(2)),
            Some(false)
        );
    }
}
