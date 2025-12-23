/// VAD configuration parameters
#[derive(Debug, Clone)]
pub struct VadConfig {
    pub vad_threshold: f32,
    pub pre_roll_samples: usize,
    pub min_speech_samples: usize,
    pub silence_to_flush_samples: usize,
    pub max_utterance_samples: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            vad_threshold: 0.5,
            pre_roll_samples: 4800,        // 300ms at 16kHz
            min_speech_samples: 4000,      // 250ms at 16kHz
            silence_to_flush_samples: 8000, // 500ms at 16kHz
            max_utterance_samples: 400000,  // 25s at 16kHz
        }
    }
}

impl VadConfig {
    /// Create config from millisecond values
    pub fn from_ms(
        vad_threshold: f32,
        pre_roll_ms: u32,
        min_speech_ms: u32,
        silence_to_flush_ms: u32,
        max_utterance_ms: u32,
    ) -> Self {
        const SAMPLES_PER_MS: usize = 16; // 16kHz
        Self {
            vad_threshold,
            pre_roll_samples: pre_roll_ms as usize * SAMPLES_PER_MS,
            min_speech_samples: min_speech_ms as usize * SAMPLES_PER_MS,
            silence_to_flush_samples: silence_to_flush_ms as usize * SAMPLES_PER_MS,
            max_utterance_samples: max_utterance_ms as usize * SAMPLES_PER_MS,
        }
    }
}

// TODO: Implement VadGatedPipeline for actual VAD processing
// This is placeholder for M2 integration

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_config_from_ms() {
        let config = VadConfig::from_ms(0.5, 300, 250, 500, 25000);
        assert_eq!(config.pre_roll_samples, 4800);
        assert_eq!(config.min_speech_samples, 4000);
    }
}
