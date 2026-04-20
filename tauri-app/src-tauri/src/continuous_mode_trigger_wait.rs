//! Encounter-detection trigger resolver.
//!
//! Runs the `select!` loop over timer / silence / manual / sensor and maintains
//! the sensor state machine (absence tracking + continuous-presence flag). The
//! detector loop in `continuous_mode.rs` calls `wait_for_trigger` at the top
//! of each iteration; the returned `TriggerOutcome` tells the caller whether
//! to proceed with a detect cycle or to `continue` without.
//!
//! LOGGER SESSION CONTRACT: this component does not touch the pipeline
//! logger. It emits events (`SensorStatus`) via the `RunContext` only.
//!
//! SENSOR STATE OWNERSHIP: `SensorLoopState` holds the four fields that
//! persist across iterations — `sensor_absent_since`, `prev_sensor_state`,
//! `sensor_continuous_present`, `sensor_available`. Every exit path writes
//! back every field it touched, so the caller sees the canonical view after
//! the call returns.
//!
//! COMPONENT: `continuous_mode_trigger_wait`.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tokio::sync::{watch, Notify};
use tracing::{info, warn};

use crate::continuous_mode_events::ContinuousModeEvent;
use crate::presence_sensor::PresenceState;
use crate::replay_bundle::ReplayBundleBuilder;
use crate::run_context::RunContext;

/// Per-run sensor state threaded through each `wait_for_trigger` call.
///
/// Every field is mutated across multiple paths in the `select!` block.
/// Callers MUST pass this via `&mut` and treat the post-call state as
/// authoritative.
pub struct SensorLoopState {
    /// When the sensor first reported `Present → Absent`. Cleared when the
    /// sensor returns to `Present` or when a split fires. Used to derive the
    /// `sensor_departed` context + `sensor_absent_secs` replay field.
    pub sensor_absent_since: Option<DateTime<Utc>>,
    /// Last sensor state observed by this detector task. Starts `Unknown`.
    pub prev_sensor_state: PresenceState,
    /// True when the sensor has shown unbroken presence since the last split.
    /// Cleared on any `Present → Absent` transition; set on the split side by
    /// the caller. The detection evaluator reads this to raise the LLM-only
    /// split threshold to 0.99 during couples/family visits.
    pub sensor_continuous_present: bool,
    /// True when a hybrid-mode sensor receiver exists and has not errored.
    /// Flipped to false on channel-closed (sensor disconnect); the caller
    /// then falls through to the LLM-only fallback branch on the next
    /// iteration.
    pub sensor_available: bool,
}

impl SensorLoopState {
    pub fn new(sensor_available: bool) -> Self {
        Self {
            sensor_absent_since: None,
            prev_sensor_state: PresenceState::Unknown,
            sensor_continuous_present: false,
            sensor_available,
        }
    }

    /// True iff the sensor is attached and last reported `Present`. Callers
    /// use this to inject `sensor_present=true` into the detection context
    /// and to arm the sensor-continuity gate after a split.
    pub fn is_currently_present(&self) -> bool {
        self.sensor_available && self.prev_sensor_state == PresenceState::Present
    }
}

/// Long-lived deps. Built once before the detector loop starts.
pub struct TriggerWaitDeps {
    pub bundle: Arc<Mutex<ReplayBundleBuilder>>,
    pub check_interval_secs: u64,
    pub is_hybrid_mode: bool,
    pub effective_sensor_mode: bool,
}

/// Per-call borrowed channel handles.
///
/// `hybrid_sensor_rx` is the only mutably borrowed channel — `watch::Receiver`
/// requires `&mut` to drive `changed()`. The two `Notify` channels are Arc'd
/// already and can share refs cheaply.
pub struct TriggerChannels<'a> {
    pub hybrid_sensor_rx: &'a mut Option<watch::Receiver<PresenceState>>,
    pub sensor_trigger_rx: &'a Arc<Notify>,
    pub silence_trigger_rx: &'a Arc<Notify>,
    pub manual_trigger_rx: &'a Arc<Notify>,
}

/// What the caller should do with the awaited result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerOutcome {
    /// Drop into the normal detect/evaluate path.
    Proceed {
        manual_triggered: bool,
        sensor_triggered: bool,
    },
    /// Sensor transition (or disconnect) that doesn't warrant a detect cycle
    /// on this iteration. Caller should `continue` to the top of the outer
    /// loop.
    ContinueNoop,
}

/// Wait for the next trigger. Mutates `state` to reflect any sensor
/// transitions observed, and logs sensor transitions into the replay bundle.
pub async fn wait_for_trigger<C: RunContext>(
    ctx: &C,
    deps: &TriggerWaitDeps,
    state: &mut SensorLoopState,
    chans: TriggerChannels<'_>,
) -> TriggerOutcome {
    let interval = tokio::time::Duration::from_secs(deps.check_interval_secs);

    if deps.is_hybrid_mode && state.sensor_available {
        // Hybrid mode with sensor: timer + silence + manual + sensor
        let Some(sensor_rx) = chans.hybrid_sensor_rx.as_mut() else {
            warn!(
                event = "trigger_wait_sensor_rx_missing",
                component = "continuous_mode_trigger_wait",
                "hybrid mode active but sensor receiver is None — downgrading to LLM-only",
            );
            state.sensor_available = false;
            return TriggerOutcome::ContinueNoop;
        };
        tokio::select! {
            _ = ctx.sleep(interval) => {
                TriggerOutcome::Proceed { manual_triggered: false, sensor_triggered: false }
            }
            _ = chans.silence_trigger_rx.notified() => {
                info!(
                    event = "trigger_wait_silence",
                    component = "continuous_mode_trigger_wait",
                    "hybrid: silence gap detected — triggering encounter check",
                );
                TriggerOutcome::Proceed { manual_triggered: false, sensor_triggered: false }
            }
            _ = chans.manual_trigger_rx.notified() => {
                info!(
                    event = "trigger_wait_manual",
                    component = "continuous_mode_trigger_wait",
                    "manual new patient trigger received",
                );
                TriggerOutcome::Proceed { manual_triggered: true, sensor_triggered: false }
            }
            result = sensor_rx.changed() => handle_sensor_change(ctx, deps, state, sensor_rx, result),
        }
    } else if deps.is_hybrid_mode {
        // Hybrid mode without sensor (disconnected): pure LLM fallback
        tokio::select! {
            _ = ctx.sleep(interval) => {
                TriggerOutcome::Proceed { manual_triggered: false, sensor_triggered: false }
            }
            _ = chans.silence_trigger_rx.notified() => {
                info!(
                    event = "trigger_wait_silence_llm_fallback",
                    component = "continuous_mode_trigger_wait",
                    "hybrid (LLM fallback): silence gap detected — triggering encounter check",
                );
                TriggerOutcome::Proceed { manual_triggered: false, sensor_triggered: false }
            }
            _ = chans.manual_trigger_rx.notified() => {
                info!(
                    event = "trigger_wait_manual",
                    component = "continuous_mode_trigger_wait",
                    "manual new patient trigger received",
                );
                TriggerOutcome::Proceed { manual_triggered: true, sensor_triggered: false }
            }
        }
    } else if deps.effective_sensor_mode {
        // Pure sensor mode: wait for sensor absence threshold OR manual trigger
        tokio::select! {
            _ = chans.sensor_trigger_rx.notified() => {
                info!(
                    event = "trigger_wait_sensor_absence",
                    component = "continuous_mode_trigger_wait",
                    "sensor: absence threshold reached — triggering encounter split",
                );
                TriggerOutcome::Proceed { manual_triggered: false, sensor_triggered: true }
            }
            _ = chans.manual_trigger_rx.notified() => {
                info!(
                    event = "trigger_wait_manual",
                    component = "continuous_mode_trigger_wait",
                    "manual new patient trigger received",
                );
                TriggerOutcome::Proceed { manual_triggered: true, sensor_triggered: false }
            }
        }
    } else {
        // LLM / Shadow mode: timer + silence + manual
        tokio::select! {
            _ = ctx.sleep(interval) => {
                TriggerOutcome::Proceed { manual_triggered: false, sensor_triggered: false }
            }
            _ = chans.silence_trigger_rx.notified() => {
                info!(
                    event = "trigger_wait_silence",
                    component = "continuous_mode_trigger_wait",
                    "silence gap detected — triggering encounter check",
                );
                TriggerOutcome::Proceed { manual_triggered: false, sensor_triggered: false }
            }
            _ = chans.manual_trigger_rx.notified() => {
                info!(
                    event = "trigger_wait_manual",
                    component = "continuous_mode_trigger_wait",
                    "manual new patient trigger received",
                );
                TriggerOutcome::Proceed { manual_triggered: true, sensor_triggered: false }
            }
        }
    }
}

fn handle_sensor_change<C: RunContext>(
    ctx: &C,
    deps: &TriggerWaitDeps,
    state: &mut SensorLoopState,
    sensor_rx: &mut watch::Receiver<PresenceState>,
    result: Result<(), watch::error::RecvError>,
) -> TriggerOutcome {
    match result {
        Ok(()) => {
            let new_state = *sensor_rx.borrow_and_update();
            let old_state = state.prev_sensor_state;
            state.prev_sensor_state = new_state;
            // Log transitions to replay bundle for forensic replay.
            if old_state != new_state {
                if let Ok(mut bundle) = deps.bundle.lock() {
                    bundle.add_sensor_transition(crate::replay_bundle::SensorTransition {
                        ts: ctx.now_utc().to_rfc3339(),
                        from: old_state.as_str().to_string(),
                        to: new_state.as_str().to_string(),
                    });
                }
            }
            match (old_state, new_state) {
                (PresenceState::Present, PresenceState::Absent) => {
                    state.sensor_absent_since = Some(ctx.now_utc());
                    state.sensor_continuous_present = false;
                    info!(
                        event = "trigger_wait_sensor_departed",
                        component = "continuous_mode_trigger_wait",
                        "hybrid: sensor detected departure (Present→Absent), accelerating LLM check",
                    );
                    // sensor_triggered → accelerate LLM check (NOT force-split)
                    TriggerOutcome::Proceed {
                        manual_triggered: false,
                        sensor_triggered: true,
                    }
                }
                (_, PresenceState::Present) => {
                    if state.sensor_absent_since.is_some() {
                        info!(
                            event = "trigger_wait_sensor_returned",
                            component = "continuous_mode_trigger_wait",
                            "hybrid: person returned — cancelling sensor absence tracking",
                        );
                        state.sensor_absent_since = None;
                    }
                    TriggerOutcome::ContinueNoop
                }
                _ => TriggerOutcome::ContinueNoop,
            }
        }
        Err(_) => {
            warn!(
                event = "trigger_wait_sensor_channel_closed",
                component = "continuous_mode_trigger_wait",
                "hybrid: sensor watch channel closed — sensor disconnected. Falling back to LLM-only.",
            );
            state.sensor_available = false;
            state.sensor_absent_since = None;
            ContinuousModeEvent::SensorStatus {
                connected: false,
                state: "unknown".into(),
            }
            .emit_via(ctx);
            TriggerOutcome::ContinueNoop
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_loop_state_defaults() {
        let s = SensorLoopState::new(true);
        assert_eq!(s.sensor_available, true);
        assert_eq!(s.sensor_continuous_present, false);
        assert!(s.sensor_absent_since.is_none());
        assert_eq!(s.prev_sensor_state, PresenceState::Unknown);
    }

    #[test]
    fn trigger_outcome_equality() {
        // Sanity: the enum is PartialEq so tests in continuous_mode.rs can
        // match on expected outcomes without a full `match` block.
        assert_eq!(
            TriggerOutcome::Proceed { manual_triggered: true, sensor_triggered: false },
            TriggerOutcome::Proceed { manual_triggered: true, sensor_triggered: false }
        );
        assert_ne!(
            TriggerOutcome::Proceed { manual_triggered: true, sensor_triggered: false },
            TriggerOutcome::ContinueNoop
        );
    }
}
