//! Emotion detection module using wav2small.
//!
//! This module provides speech emotion recognition using the wav2small model,
//! which is ultra-lightweight (~120KB) and outputs dimensional emotion values
//! (arousal, dominance, valence).

mod provider;

pub use provider::{EmotionConfig, EmotionError, EmotionProvider, EmotionResult};
