//! Loop-local state shared across continuous-mode detector iterations.
//!
//! `LoopState` holds the two counters that multiple detector-task phases
//! need to read and mutate — `encounter_number` (monotonic per-run encounter
//! sequence) and `merge_back_count` (how many consecutive splits have been
//! merged back into the previous encounter, used for confidence-threshold
//! escalation). Extracted from the detector task so the merge-back
//! coordinator can mutate these in-place without a huge tuple return.
//!
//! Kept intentionally small: only fields that are read and written across
//! the detector loop + its extracted coordinators belong here. Fields used
//! only within a single iteration stay as local bindings.

/// Per-run mutable loop state for the continuous-mode detector task.
pub struct LoopState {
    /// Monotonic counter for encounters split within this continuous-mode
    /// run. Incremented on each split, decremented on merge-back (via
    /// `saturating_sub(1)` to avoid underflow on pathological input).
    pub encounter_number: u32,
    /// Number of consecutive splits that were merged back into the previous
    /// encounter since the last confirmed standalone split. Each merge-back
    /// escalates the confidence threshold for the next detection by +0.05
    /// (capped at 0.99); reset to 0 when a split "sticks".
    pub merge_back_count: u32,
}

impl LoopState {
    pub fn new() -> Self {
        Self {
            encounter_number: 0,
            merge_back_count: 0,
        }
    }
}

impl Default for LoopState {
    fn default() -> Self {
        Self::new()
    }
}
