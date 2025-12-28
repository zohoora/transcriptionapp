//! Biomarker configuration

use std::path::PathBuf;

/// Configuration for the biomarker analysis system
#[derive(Debug, Clone)]
pub struct BiomarkerConfig {
    /// Enable YAMNet cough detection
    pub cough_detection_enabled: bool,
    /// Path to YAMNet ONNX model
    pub yamnet_model_path: Option<PathBuf>,
    /// Confidence threshold for cough detection (0.0-1.0)
    pub cough_threshold: f32,

    /// Enable vitality metric (pitch variability)
    pub vitality_enabled: bool,
    /// Enable stability metric (CPP)
    pub stability_enabled: bool,

    /// Enable session metrics aggregation
    pub session_metrics_enabled: bool,

    /// Number of threads for ONNX inference
    pub n_threads: usize,
}

impl Default for BiomarkerConfig {
    fn default() -> Self {
        Self {
            cough_detection_enabled: true,
            yamnet_model_path: None,
            cough_threshold: 0.5,
            vitality_enabled: true,
            stability_enabled: true,
            session_metrics_enabled: true,
            n_threads: 1,
        }
    }
}

impl BiomarkerConfig {
    /// Check if any biomarker analysis is enabled
    pub fn any_enabled(&self) -> bool {
        self.cough_detection_enabled
            || self.vitality_enabled
            || self.stability_enabled
            || self.session_metrics_enabled
    }

    /// Check if YAMNet is ready (model available)
    pub fn yamnet_ready(&self) -> bool {
        self.cough_detection_enabled
            && self.yamnet_model_path
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BiomarkerConfig::default();
        assert!(config.cough_detection_enabled);
        assert!(config.vitality_enabled);
        assert!(config.stability_enabled);
        assert!(config.any_enabled());
    }

    #[test]
    fn test_yamnet_not_ready_without_path() {
        let config = BiomarkerConfig::default();
        assert!(!config.yamnet_ready());
    }
}
