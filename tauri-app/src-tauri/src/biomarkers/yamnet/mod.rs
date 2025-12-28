//! YAMNet cough detection module
//!
//! Uses YAMNet ONNX model to detect coughs and other audio events in
//! continuous audio streams.
//!
//! ## Implementation
//! - Sliding window: 1 second (16000 samples) with 500ms hop (8000 samples)
//! - Class 47 = Cough in YAMNet's 521 audio classes
//! - Configurable confidence threshold
//!
//! ## Model
//! YAMNet-lite (~3MB ONNX) from TensorFlow Model Garden

mod sliding_window;

use anyhow::Result;
use std::path::Path;
use tracing::info;

use super::CoughEvent;
pub use sliding_window::SlidingWindow;

#[cfg(feature = "biomarkers")]
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};

/// YAMNet class IDs for audio events we care about
const CLASS_COUGH: usize = 47;
const CLASS_THROAT_CLEARING: usize = 48;
const CLASS_SNEEZE: usize = 49;

/// YAMNet audio event classifier
#[cfg(feature = "biomarkers")]
pub struct YamnetProvider {
    session: Session,
    sliding_window: SlidingWindow,
}

#[cfg(feature = "biomarkers")]
impl YamnetProvider {
    /// Create a new YAMNet provider
    pub fn new(model_path: &Path, n_threads: usize) -> Result<Self> {
        info!("Loading YAMNet model from {:?}", model_path);

        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create session builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Failed to set optimization level: {}", e))?
            .with_intra_threads(n_threads)
            .map_err(|e| anyhow::anyhow!("Failed to set threads: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {}", e))?;

        info!("YAMNet model loaded successfully");

        Ok(Self {
            session,
            sliding_window: SlidingWindow::new(),
        })
    }

    /// Process an audio chunk and return any detected cough events
    pub fn process_chunk(
        &mut self,
        samples: &[f32],
        timestamp_ms: u64,
        threshold: f32,
    ) -> Result<Vec<CoughEvent>> {
        let mut events = Vec::new();

        // Add samples to sliding window
        self.sliding_window.add_samples(samples);

        // Process any complete windows
        while let Some((window, window_start_offset)) = self.sliding_window.next_window() {
            // Calculate timestamp for this window
            let window_timestamp_ms =
                timestamp_ms.saturating_sub((samples.len() as u64 * 1000) / 16000)
                    + (window_start_offset as u64 * 1000) / 16000;

            // Run inference
            let predictions = self.infer(&window)?;

            // Check for cough events
            if predictions.len() > CLASS_COUGH && predictions[CLASS_COUGH] > threshold {
                events.push(CoughEvent {
                    timestamp_ms: window_timestamp_ms,
                    duration_ms: 1000, // 1 second window
                    confidence: predictions[CLASS_COUGH],
                    label: "Cough".to_string(),
                });
            }

            if predictions.len() > CLASS_THROAT_CLEARING && predictions[CLASS_THROAT_CLEARING] > threshold {
                events.push(CoughEvent {
                    timestamp_ms: window_timestamp_ms,
                    duration_ms: 1000,
                    confidence: predictions[CLASS_THROAT_CLEARING],
                    label: "Throat clearing".to_string(),
                });
            }

            if predictions.len() > CLASS_SNEEZE && predictions[CLASS_SNEEZE] > threshold {
                events.push(CoughEvent {
                    timestamp_ms: window_timestamp_ms,
                    duration_ms: 1000,
                    confidence: predictions[CLASS_SNEEZE],
                    label: "Sneeze".to_string(),
                });
            }
        }

        Ok(events)
    }

    /// Run YAMNet inference on a 1-second window
    fn infer(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        // YAMNet expects [batch, samples] = [1, 16000]
        let input_tensor = Value::from_array(([1_usize, samples.len()], samples.to_vec()))
            .map_err(|e| anyhow::anyhow!("Failed to create input tensor: {}", e))?;

        let outputs = self.session
            .run(ort::inputs![input_tensor])
            .map_err(|e| anyhow::anyhow!("Inference failed: {}", e))?;

        // Output is [batch, num_classes] = [1, 521]
        let output = outputs
            .iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No output from YAMNet"))?;

        let tensor = output.1
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to extract tensor: {}", e))?;

        let predictions: Vec<f32> = tensor.1.iter().copied().collect();

        Ok(predictions)
    }
}

/// Stub for when biomarkers feature is disabled
#[cfg(not(feature = "biomarkers"))]
pub struct YamnetProvider;

#[cfg(not(feature = "biomarkers"))]
impl YamnetProvider {
    pub fn new(_model_path: &Path, _n_threads: usize) -> Result<Self> {
        anyhow::bail!("YAMNet requires the 'biomarkers' feature")
    }

    pub fn process_chunk(
        &mut self,
        _samples: &[f32],
        _timestamp_ms: u64,
        _threshold: f32,
    ) -> Result<Vec<CoughEvent>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sliding_window_creation() {
        let mut window = SlidingWindow::new();
        assert!(window.next_window().is_none());
    }
}
