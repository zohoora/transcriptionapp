//! Speech enhancement module using GTCRN.
//!
//! This module provides audio denoising/enhancement using the GTCRN
//! (Grouped Temporal Convolutional Recurrent Network) model, which is
//! ultra-lightweight (~523KB) and runs in real-time.

mod provider;

pub use provider::{EnhancementConfig, EnhancementError, EnhancementProvider};
