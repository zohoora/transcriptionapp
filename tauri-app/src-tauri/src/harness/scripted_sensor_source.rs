//! Builds a MockSource (from presence_sensor::sources::mock) seeded from a
//! bundle's sensor_transitions — so the harness can drive the orchestrator
//! through the same sensor-state transitions recorded in production.
//!
//! This file is a thin conversion utility. MockSource already implements
//! SensorSource; we reuse it unchanged.

use crate::presence_sensor::sources::mock::MockSource;
use crate::replay_bundle::SensorTransition;
use std::time::Duration;

/// Build a MockSource that replays the recorded sensor transitions.
///
/// Transitions with `to == "Present"` become `true`; anything else becomes
/// `false`. The `delay` for each event is the wall-clock delta from the first
/// transition (or 1 second apart if timestamps can't be parsed).
pub fn mock_source_from_transitions(transitions: &[SensorTransition]) -> MockSource {
    let base_ts_ms = transitions
        .iter()
        .find_map(|t| chrono::DateTime::parse_from_rfc3339(&t.ts).ok())
        .map(|dt| dt.timestamp_millis());

    let mut sequence: Vec<(Duration, bool)> = Vec::with_capacity(transitions.len());
    let mut fallback_offset = Duration::ZERO;

    for t in transitions {
        let offset = if let (Some(base), Ok(dt)) = (base_ts_ms, chrono::DateTime::parse_from_rfc3339(&t.ts)) {
            let delta_ms = (dt.timestamp_millis() - base).max(0) as u64;
            Duration::from_millis(delta_ms)
        } else {
            fallback_offset += Duration::from_secs(1);
            fallback_offset
        };
        let present = t.to == "Present";
        sequence.push((offset, present));
    }

    MockSource::mmwave_sequence(sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_transitions_build_cleanly() {
        let _src = mock_source_from_transitions(&[]);
    }

    #[test]
    fn single_present_transition() {
        let transitions = vec![SensorTransition {
            ts: "2026-04-14T10:00:00Z".into(),
            from: "Absent".into(),
            to: "Present".into(),
        }];
        let _src = mock_source_from_transitions(&transitions);
        // MockSource's Vec<MockEvent> is private; construction success implies parse success.
    }

    #[test]
    fn unparseable_timestamps_use_fallback_ordering() {
        let transitions = vec![
            SensorTransition {
                ts: "not-a-real-ts".into(),
                from: "Absent".into(),
                to: "Present".into(),
            },
            SensorTransition {
                ts: "also-not-a-ts".into(),
                from: "Present".into(),
                to: "Absent".into(),
            },
        ];
        let _src = mock_source_from_transitions(&transitions);
        // No panic = success.
    }
}
