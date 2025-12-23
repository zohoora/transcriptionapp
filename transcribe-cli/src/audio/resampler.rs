use anyhow::{Context, Result};
use rubato::{FftFixedIn, Resampler};
use tracing::debug;

/// Target sample rate for VAD and Whisper
pub const TARGET_SAMPLE_RATE: u32 = 16000;

/// Audio resampler wrapper for converting device sample rate to 16kHz
pub struct AudioResampler {
    resampler: FftFixedIn<f32>,
    input_buffer: Vec<Vec<f32>>,
    output_buffer: Vec<Vec<f32>>,
    input_frames: usize,
}

impl AudioResampler {
    /// Create a new resampler from device sample rate to 16kHz
    pub fn new(device_sample_rate: u32) -> Result<Self> {
        let ratio = TARGET_SAMPLE_RATE as f64 / device_sample_rate as f64;

        debug!(
            "Creating resampler: {} Hz -> {} Hz (ratio: {:.4})",
            device_sample_rate, TARGET_SAMPLE_RATE, ratio
        );

        // Use 1024 input frames as a reasonable chunk size
        let input_frames = 1024;
        let channels = 1; // Mono

        let resampler = FftFixedIn::new(
            device_sample_rate as usize,
            TARGET_SAMPLE_RATE as usize,
            input_frames,
            2, // sub_chunks for quality
            channels,
        )
        .context("Failed to create resampler")?;

        // Pre-allocate buffers
        let input_buffer = vec![vec![0.0f32; input_frames]; channels];
        let output_buffer = resampler.output_buffer_allocate(true);

        Ok(Self {
            resampler,
            input_buffer,
            output_buffer,
            input_frames,
        })
    }

    /// Get the number of input frames needed for the next process call
    pub fn input_frames_next(&self) -> usize {
        self.input_frames
    }

    /// Process input samples and return resampled output
    ///
    /// Input must be exactly `input_frames_next()` samples.
    /// Returns resampled samples at 16kHz.
    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        if input.len() != self.input_frames {
            anyhow::bail!(
                "Input length {} doesn't match expected {}",
                input.len(),
                self.input_frames
            );
        }

        // Copy input to buffer
        self.input_buffer[0].copy_from_slice(input);

        // Process
        let (_, output_frames) = self
            .resampler
            .process_into_buffer(&self.input_buffer, &mut self.output_buffer, None)
            .context("Resampling failed")?;

        // Extract output
        Ok(self.output_buffer[0][..output_frames].to_vec())
    }

    /// Reset the resampler state
    pub fn reset(&mut self) {
        self.resampler.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resampler_48k_to_16k() {
        let mut resampler = AudioResampler::new(48000).expect("Failed to create resampler");

        // FFT-based resamplers have latency, so process multiple chunks
        let input = vec![0.0f32; resampler.input_frames_next()];
        let mut total_output = 0;
        let mut total_input = 0;

        // Process several chunks to get past the initial latency
        for _ in 0..5 {
            let output = resampler.process(&input).expect("Resampling failed");
            total_output += output.len();
            total_input += input.len();
        }

        // The resampler produces output, and ratio should be approximately correct
        let expected_ratio = 16000.0 / 48000.0;
        let actual_ratio = total_output as f64 / total_input as f64;
        assert!(
            (actual_ratio - expected_ratio).abs() < 0.1,
            "Expected ratio ~{:.3}, got {:.3}",
            expected_ratio,
            actual_ratio
        );
    }

    #[test]
    fn test_resampler_44100_to_16k() {
        let mut resampler = AudioResampler::new(44100).expect("Failed to create resampler");

        let input = vec![0.0f32; resampler.input_frames_next()];
        let output = resampler.process(&input).expect("Resampling failed");

        // The resampler produces output, and ratio should be approximately correct
        let expected_ratio = 16000.0 / 44100.0;
        let actual_ratio = output.len() as f64 / input.len() as f64;
        assert!(
            (actual_ratio - expected_ratio).abs() < 0.1,
            "Expected ratio ~{:.3}, got {:.3} (output len: {})",
            expected_ratio,
            actual_ratio,
            output.len()
        );
    }
}
