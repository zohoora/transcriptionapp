//! Offline test harness for run_continuous_mode.
//!
//! Drives the real orchestrator function through RecordingRunContext, seeded
//! from archived replay_bundle.json data. Intended for per-encounter and
//! per-day equivalence testing before/after refactors of continuous_mode.rs.
//!
//! See: docs/superpowers/specs/2026-04-18-continuous-mode-test-harness-design.md

pub mod captured_event;
pub mod policies;
pub mod mismatch_report;
pub mod replay_llm_backend;
pub mod scripted_sensor_source;
pub mod recording_run_context;
pub mod archive_comparator;
pub mod event_comparator;
pub mod driver;
pub mod encounter_harness;
pub mod day_harness;

pub use captured_event::CapturedEvent;
pub use encounter_harness::EncounterHarness;
pub use mismatch_report::{MismatchKind, MismatchReport, Verdict};
pub use policies::{EquivalencePolicy, PromptPolicy};
