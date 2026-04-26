//! Shared experiment harness for SOAP and billing prompt-engineering CLIs (v0.10.62+).
//!
//! Background: the Apr 24 prompt-engineering loop required ~1,500 lines of
//! Python because no Rust scaffold existed for "run prompt variant V × seeds N
//! across sessions S, score against labels". This module provides the four
//! pieces that recur across SOAP and billing experiments:
//!   - [`variant`] — `Variant` + `VariantSource` for hardcoded / file / inline prompts.
//!   - [`labels`] — load `tests/fixtures/labels/*.json` ground truth by session.
//!   - [`report`] — aggregate scoring (TP/TN/FP/FN, precision/recall) + JSON output.
//!   - [`runner`] — multi-seed orchestration trait (`Runner`).
//!
//! The two new CLIs (`soap_experiment_cli` + `billing_experiment_cli`) each
//! implement `Runner` and let this module handle the seed × variant × session
//! cross product. Existing CLIs (`encounter_experiment_cli`, `vision_experiment_cli`)
//! can adopt the harness incrementally — auto-loading labels via `labels::for_session`
//! is the first migration target.

pub mod variant;
pub mod labels;
pub mod report;
pub mod runner;

pub use variant::{Variant, VariantSource, parse_variant_arg};
pub use labels::{LabelEntry, load_label_for_session};
pub use report::{
    Score, AggregateScore, LatencySummary, latency_per_variant, write_results_json,
    write_performance_summary_json,
};
