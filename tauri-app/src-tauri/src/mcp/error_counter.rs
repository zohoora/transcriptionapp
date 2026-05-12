//! Rolling 1-hour error counter for the MCP `get_status` endpoint.
//!
//! Subscribes to the global tracing dispatcher via a `Layer`; every
//! `Level::ERROR` event increments the counter. The MCP status handler reads
//! `count_last_hour()` to surface a rough health signal to operators.
//!
//! Memory-bounded: prunes timestamps older than one hour on every record/read.
//! Hard cap of 10,000 entries protects against pathological burst rates.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

const WINDOW: Duration = Duration::from_secs(3600);
const HARD_CAP: usize = 10_000;

#[derive(Default)]
pub struct ErrorCounter {
    timestamps: Mutex<VecDeque<Instant>>,
}

impl ErrorCounter {
    pub fn record(&self) {
        let Ok(mut q) = self.timestamps.lock() else { return };
        let now = Instant::now();
        prune_older_than(&mut q, now);
        if q.len() >= HARD_CAP {
            q.pop_front();
        }
        q.push_back(now);
    }

    pub fn count_last_hour(&self) -> u32 {
        let Ok(mut q) = self.timestamps.lock() else { return 0 };
        prune_older_than(&mut q, Instant::now());
        q.len().min(u32::MAX as usize) as u32
    }
}

fn prune_older_than(q: &mut VecDeque<Instant>, now: Instant) {
    let cutoff = now.checked_sub(WINDOW);
    let Some(cutoff) = cutoff else { return };
    while let Some(&front) = q.front() {
        if front < cutoff {
            q.pop_front();
        } else {
            break;
        }
    }
}

static GLOBAL: OnceLock<Arc<ErrorCounter>> = OnceLock::new();

/// Returns the process-wide counter, initialising the global on first call.
/// Safe to call from any thread.
pub fn global() -> Arc<ErrorCounter> {
    GLOBAL
        .get_or_init(|| Arc::new(ErrorCounter::default()))
        .clone()
}

pub struct ErrorCounterLayer {
    counter: Arc<ErrorCounter>,
}

impl ErrorCounterLayer {
    pub fn new(counter: Arc<ErrorCounter>) -> Self {
        Self { counter }
    }
}

impl<S: Subscriber> Layer<S> for ErrorCounterLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if *event.metadata().level() == Level::ERROR {
            self.counter.record();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_counter_returns_zero() {
        let c = ErrorCounter::default();
        assert_eq!(c.count_last_hour(), 0);
    }

    #[test]
    fn record_increments_count() {
        let c = ErrorCounter::default();
        c.record();
        c.record();
        c.record();
        assert_eq!(c.count_last_hour(), 3);
    }

    #[test]
    fn prunes_entries_older_than_window() {
        let c = ErrorCounter::default();
        {
            let mut q = c.timestamps.lock().unwrap();
            let stale = Instant::now() - Duration::from_secs(3700);
            q.push_back(stale);
            q.push_back(stale);
        }
        c.record();
        assert_eq!(c.count_last_hour(), 1);
    }

    #[test]
    fn hard_cap_bounds_memory() {
        let c = ErrorCounter::default();
        for _ in 0..(HARD_CAP + 100) {
            c.record();
        }
        assert!(c.count_last_hour() as usize <= HARD_CAP);
    }
}
