//! Voice metrics: Vitality and Stability
//!
//! These metrics analyze vocal characteristics for health monitoring:
//!
//! - **Vitality** (Prosody/Emotional Engagement):
//!   Measures pitch variability to detect "flat affect" (Depression/PTSD).
//!   Uses F0 standard deviation via the mcleod pitch detection algorithm.
//!
//! - **Stability** (Neurological Control):
//!   Measures vocal fold regularity to detect fatigue or tremors (Parkinson's).
//!   Uses CPP (Cepstral Peak Prominence), NOT jitter/shimmer (which fail in ambient noise).

mod vitality;
mod stability;

pub use vitality::calculate_vitality;
pub use stability::calculate_stability;
