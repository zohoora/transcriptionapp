//! Audio Quality Analysis Module
//!
//! Provides real-time audio quality metrics for predicting transcript reliability.
//! These metrics don't change the audio - they tell you when your transcript is likely
//! garbage, enabling downstream NLP to weight results by confidence.
//!
//! ## Metrics
//!
//! **Tier 1 - Ultra-cheap (O(1) per sample):**
//! - Peak level (dBFS)
//! - RMS level (dBFS)
//! - Clipping count
//! - Dropout counter
//!
//! **Tier 2 - Cheap (O(N) per chunk):**
//! - Noise floor estimate
//! - SNR estimate
//! - Silence ratio

use std::collections::VecDeque;

/// Sample rate for audio quality analysis (matches pipeline)
const SAMPLE_RATE: usize = 16000;

/// RMS window size in samples (300ms at 16kHz = 4800 samples)
const RMS_WINDOW_SAMPLES: usize = SAMPLE_RATE * 300 / 1000;

/// Clipping threshold (samples at or above this are considered clipped)
const CLIPPING_THRESHOLD: f32 = 0.98;

/// Minimum noise floor to avoid division by zero (corresponds to ~-60 dBFS)
const MIN_NOISE_FLOOR: f32 = 0.001;

/// Snapshot emission interval in milliseconds
const SNAPSHOT_INTERVAL_MS: u64 = 500;

/// Audio quality snapshot - emitted periodically
#[derive(Debug, Clone)]
pub struct AudioQualitySnapshot {
    pub timestamp_ms: u64,

    // Tier 1 - per chunk
    pub peak_db: f32,
    pub rms_db: f32,
    pub clipped_samples: u32,
    pub clipped_ratio: f32,

    // Tier 2 - running estimates
    pub noise_floor_db: f32,
    pub snr_db: f32,
    pub silence_ratio: f32,

    // Counters
    pub dropout_count: u32,
    pub total_clipped: u32,
    pub total_samples: u64,
}

/// Audio quality flags - derived from snapshot
#[derive(Debug, Clone)]
pub struct AudioQualityFlags {
    pub level_ok: bool,
    pub clipping_ok: bool,
    pub snr_ok: bool,
    pub dropout_ok: bool,
    pub overall_quality: f32,
}

/// Analyzer for computing audio quality metrics in real-time
pub struct AudioQualityAnalyzer {
    // Tier 1 state - RMS calculation
    rms_window: VecDeque<f32>,
    rms_sum: f32,

    // Tier 1 state - per-chunk tracking
    peak_this_chunk: f32,
    clipped_this_chunk: u32,
    total_clipped: u32,
    samples_this_chunk: u32,

    // Tier 2 state - noise/speech energy
    noise_energy_sum: f32,
    noise_frame_count: u32,
    speech_energy_sum: f32,
    speech_frame_count: u32,

    // Tier 2 state - silence tracking
    silence_frames: u32,
    total_frames: u32,

    // Session tracking
    dropout_count: u32,
    total_samples: u64,

    // Snapshot timing
    last_snapshot_ms: u64,
}

impl AudioQualityAnalyzer {
    /// Create a new audio quality analyzer
    pub fn new() -> Self {
        Self {
            rms_window: VecDeque::with_capacity(RMS_WINDOW_SAMPLES),
            rms_sum: 0.0,
            peak_this_chunk: 0.0,
            clipped_this_chunk: 0,
            total_clipped: 0,
            samples_this_chunk: 0,
            noise_energy_sum: 0.0,
            noise_frame_count: 0,
            speech_energy_sum: 0.0,
            speech_frame_count: 0,
            silence_frames: 0,
            total_frames: 0,
            dropout_count: 0,
            total_samples: 0,
            last_snapshot_ms: 0,
        }
    }

    /// Process a chunk of audio samples
    ///
    /// Returns a snapshot every ~500ms, otherwise None
    pub fn process_chunk(
        &mut self,
        samples: &[f32],
        timestamp_ms: u64,
        is_speech: bool,
    ) -> Option<AudioQualitySnapshot> {
        // Process each sample
        for &sample in samples {
            self.process_sample(sample);
        }

        // Update total samples
        self.total_samples += samples.len() as u64;
        self.samples_this_chunk += samples.len() as u32;

        // Calculate chunk RMS for noise/speech tracking
        let chunk_rms = self.calculate_chunk_rms(samples);

        // Update noise/speech estimates based on VAD
        self.total_frames += 1;
        if is_speech {
            self.speech_energy_sum += chunk_rms;
            self.speech_frame_count += 1;
        } else {
            self.noise_energy_sum += chunk_rms;
            self.noise_frame_count += 1;
            self.silence_frames += 1;
        }

        // Check if it's time to emit a snapshot
        if timestamp_ms >= self.last_snapshot_ms + SNAPSHOT_INTERVAL_MS {
            let snapshot = self.create_snapshot(timestamp_ms);
            self.last_snapshot_ms = timestamp_ms;
            self.reset_chunk_counters();
            Some(snapshot)
        } else {
            None
        }
    }

    /// Record a dropout event (buffer overflow/underrun)
    pub fn record_dropout(&mut self) {
        self.dropout_count += 1;
    }

    /// Get current quality flags based on accumulated data
    pub fn get_flags(&self) -> AudioQualityFlags {
        let rms_db = self.calculate_rms_db();
        let snr_db = self.calculate_snr_db();
        let clipped_ratio = if self.total_samples > 0 {
            self.total_clipped as f32 / self.total_samples as f32
        } else {
            0.0
        };

        // Level is OK if RMS is in -40 to -6 dBFS range
        let level_ok = rms_db >= -40.0 && rms_db <= -6.0;

        // Clipping is OK if < 0.1% clipped
        let clipping_ok = clipped_ratio < 0.001;

        // SNR is OK if > 10 dB
        let snr_ok = snr_db > 10.0;

        // Dropout is OK if no overruns
        let dropout_ok = self.dropout_count == 0;

        // Overall quality score (0.0 - 1.0)
        let overall_quality = self.calculate_overall_quality(level_ok, clipping_ok, snr_ok, dropout_ok);

        AudioQualityFlags {
            level_ok,
            clipping_ok,
            snr_ok,
            dropout_ok,
            overall_quality,
        }
    }

    /// Reset the analyzer for a new session
    pub fn reset(&mut self) {
        self.rms_window.clear();
        self.rms_sum = 0.0;
        self.peak_this_chunk = 0.0;
        self.clipped_this_chunk = 0;
        self.total_clipped = 0;
        self.samples_this_chunk = 0;
        self.noise_energy_sum = 0.0;
        self.noise_frame_count = 0;
        self.speech_energy_sum = 0.0;
        self.speech_frame_count = 0;
        self.silence_frames = 0;
        self.total_frames = 0;
        self.dropout_count = 0;
        self.total_samples = 0;
        self.last_snapshot_ms = 0;
    }

    // --- Private methods ---

    /// Process a single sample for RMS, peak, and clipping
    fn process_sample(&mut self, sample: f32) {
        let abs_sample = sample.abs();

        // Update peak
        if abs_sample > self.peak_this_chunk {
            self.peak_this_chunk = abs_sample;
        }

        // Check for clipping
        if abs_sample >= CLIPPING_THRESHOLD {
            self.clipped_this_chunk += 1;
            self.total_clipped += 1;
        }

        // Update RMS window (squared values)
        let squared = sample * sample;
        self.rms_sum += squared;
        self.rms_window.push_back(squared);

        // Remove oldest if window is full
        if self.rms_window.len() > RMS_WINDOW_SAMPLES {
            if let Some(old) = self.rms_window.pop_front() {
                self.rms_sum -= old;
            }
        }
    }

    /// Calculate RMS for a chunk (for noise/speech tracking)
    fn calculate_chunk_rms(&self, samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    /// Calculate current RMS in dBFS
    fn calculate_rms_db(&self) -> f32 {
        if self.rms_window.is_empty() {
            return -60.0;
        }
        let rms = (self.rms_sum / self.rms_window.len() as f32).sqrt();
        amplitude_to_db(rms)
    }

    /// Calculate SNR in dB
    fn calculate_snr_db(&self) -> f32 {
        let noise_floor = if self.noise_frame_count > 0 {
            self.noise_energy_sum / self.noise_frame_count as f32
        } else {
            MIN_NOISE_FLOOR
        };

        let speech_level = if self.speech_frame_count > 0 {
            self.speech_energy_sum / self.speech_frame_count as f32
        } else {
            0.0
        };

        // Avoid division by zero
        let noise_floor = noise_floor.max(MIN_NOISE_FLOOR);

        if speech_level > 0.0 {
            20.0 * (speech_level / noise_floor).log10()
        } else {
            0.0
        }
    }

    /// Create a snapshot of current quality metrics
    fn create_snapshot(&self, timestamp_ms: u64) -> AudioQualitySnapshot {
        let rms_db = self.calculate_rms_db();
        let peak_db = amplitude_to_db(self.peak_this_chunk);
        let snr_db = self.calculate_snr_db();

        let noise_floor_db = if self.noise_frame_count > 0 {
            let noise_rms = self.noise_energy_sum / self.noise_frame_count as f32;
            amplitude_to_db(noise_rms)
        } else {
            -60.0
        };

        let silence_ratio = if self.total_frames > 0 {
            self.silence_frames as f32 / self.total_frames as f32
        } else {
            0.0
        };

        let clipped_ratio = if self.samples_this_chunk > 0 {
            self.clipped_this_chunk as f32 / self.samples_this_chunk as f32
        } else {
            0.0
        };

        AudioQualitySnapshot {
            timestamp_ms,
            peak_db,
            rms_db,
            clipped_samples: self.clipped_this_chunk,
            clipped_ratio,
            noise_floor_db,
            snr_db,
            silence_ratio,
            dropout_count: self.dropout_count,
            total_clipped: self.total_clipped,
            total_samples: self.total_samples,
        }
    }

    /// Reset per-chunk counters after emitting a snapshot
    fn reset_chunk_counters(&mut self) {
        self.peak_this_chunk = 0.0;
        self.clipped_this_chunk = 0;
        self.samples_this_chunk = 0;
    }

    /// Calculate overall quality score (0.0 - 1.0)
    fn calculate_overall_quality(
        &self,
        level_ok: bool,
        clipping_ok: bool,
        snr_ok: bool,
        dropout_ok: bool,
    ) -> f32 {
        let mut score = 0.0;
        let mut weight = 0.0;

        // Level: 25% weight
        if level_ok {
            score += 0.25;
        }
        weight += 0.25;

        // Clipping: 25% weight (critical)
        if clipping_ok {
            score += 0.25;
        }
        weight += 0.25;

        // SNR: 25% weight
        if snr_ok {
            score += 0.25;
        }
        weight += 0.25;

        // Dropout: 25% weight (critical)
        if dropout_ok {
            score += 0.25;
        }
        weight += 0.25;

        if weight > 0.0 {
            score / weight
        } else {
            0.0
        }
    }
}

impl Default for AudioQualityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert amplitude (0.0 - 1.0) to dBFS
fn amplitude_to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        -60.0 // Floor at -60 dBFS
    } else {
        20.0 * amplitude.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_analyzer() {
        let analyzer = AudioQualityAnalyzer::new();
        assert_eq!(analyzer.total_samples, 0);
        assert_eq!(analyzer.dropout_count, 0);
    }

    #[test]
    fn test_amplitude_to_db() {
        // 1.0 amplitude = 0 dBFS
        assert!((amplitude_to_db(1.0) - 0.0).abs() < 0.01);

        // 0.5 amplitude = -6 dBFS (approximately)
        assert!((amplitude_to_db(0.5) - (-6.02)).abs() < 0.1);

        // 0.1 amplitude = -20 dBFS
        assert!((amplitude_to_db(0.1) - (-20.0)).abs() < 0.1);

        // 0.0 amplitude = -60 dBFS (floor)
        assert_eq!(amplitude_to_db(0.0), -60.0);
    }

    #[test]
    fn test_clipping_detection() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // Create samples with some clipping
        let mut samples = vec![0.5; 1000];
        samples[500] = 0.99; // clipped
        samples[501] = 1.0;  // clipped
        samples[502] = -0.98; // clipped

        analyzer.process_chunk(&samples, 0, true);

        assert_eq!(analyzer.total_clipped, 3);
    }

    #[test]
    fn test_silence_detection() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // Process some silence frames
        let silence = vec![0.001; 512];
        for i in 0..10 {
            analyzer.process_chunk(&silence, i as u64 * 32, false);
        }

        // Process some speech frames
        let speech = vec![0.3; 512];
        for i in 10..15 {
            analyzer.process_chunk(&speech, i as u64 * 32, true);
        }

        let flags = analyzer.get_flags();
        assert!(analyzer.silence_frames > 0);
        assert!(analyzer.speech_frame_count > 0);
        assert!(flags.snr_ok); // Should have good SNR
    }

    #[test]
    fn test_dropout_recording() {
        let mut analyzer = AudioQualityAnalyzer::new();

        assert_eq!(analyzer.dropout_count, 0);

        analyzer.record_dropout();
        analyzer.record_dropout();

        assert_eq!(analyzer.dropout_count, 2);

        let flags = analyzer.get_flags();
        assert!(!flags.dropout_ok);
    }

    #[test]
    fn test_snapshot_timing() {
        let mut analyzer = AudioQualityAnalyzer::new();
        let samples = vec![0.3; 512];

        // First call should not produce snapshot (not enough time elapsed)
        let snapshot = analyzer.process_chunk(&samples, 0, true);
        assert!(snapshot.is_none());

        // Call at 500ms should produce snapshot
        let snapshot = analyzer.process_chunk(&samples, 500, true);
        assert!(snapshot.is_some());

        // Call at 600ms should not produce snapshot (not 500ms since last)
        let snapshot = analyzer.process_chunk(&samples, 600, true);
        assert!(snapshot.is_none());

        // Call at 1000ms should produce snapshot
        let snapshot = analyzer.process_chunk(&samples, 1000, true);
        assert!(snapshot.is_some());
    }

    #[test]
    fn test_quality_flags_good_audio() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // Simulate good audio: moderate level, no clipping, good SNR
        let speech = vec![0.2; 512]; // ~-14 dBFS
        let silence = vec![0.01; 512]; // ~-40 dBFS

        // Add some silence frames for noise floor
        for i in 0..5 {
            analyzer.process_chunk(&silence, i as u64 * 32, false);
        }

        // Add speech frames
        for i in 5..20 {
            analyzer.process_chunk(&speech, i as u64 * 32, true);
        }

        let flags = analyzer.get_flags();
        assert!(flags.level_ok);
        assert!(flags.clipping_ok);
        assert!(flags.dropout_ok);
        assert!(flags.overall_quality >= 0.75);
    }

    #[test]
    fn test_reset() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // Process some audio
        let samples = vec![0.5; 1000];
        analyzer.process_chunk(&samples, 0, true);
        analyzer.record_dropout();

        assert!(analyzer.total_samples > 0);
        assert_eq!(analyzer.dropout_count, 1);

        // Reset
        analyzer.reset();

        assert_eq!(analyzer.total_samples, 0);
        assert_eq!(analyzer.dropout_count, 0);
        assert!(analyzer.rms_window.is_empty());
    }

    #[test]
    fn test_too_quiet_audio() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // Very quiet audio: ~-50 dBFS (below -40 threshold)
        let quiet = vec![0.003; 512];
        for i in 0..10 {
            analyzer.process_chunk(&quiet, i as u64 * 32, true);
        }

        let flags = analyzer.get_flags();
        assert!(!flags.level_ok, "Level should not be OK for very quiet audio");
    }

    #[test]
    fn test_too_loud_audio() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // Very loud audio: ~-3 dBFS (above -6 threshold)
        let loud = vec![0.7; 512];
        for i in 0..10 {
            analyzer.process_chunk(&loud, i as u64 * 32, true);
        }

        let flags = analyzer.get_flags();
        assert!(!flags.level_ok, "Level should not be OK for very loud audio");
    }

    #[test]
    fn test_noisy_environment_low_snr() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // High noise floor
        let noise = vec![0.15; 512]; // ~-16 dBFS noise
        for i in 0..10 {
            analyzer.process_chunk(&noise, i as u64 * 32, false);
        }

        // Speech only slightly louder than noise
        let speech = vec![0.2; 512]; // ~-14 dBFS speech
        for i in 10..20 {
            analyzer.process_chunk(&speech, i as u64 * 32, true);
        }

        let flags = analyzer.get_flags();
        assert!(!flags.snr_ok, "SNR should not be OK for noisy environment");
    }

    #[test]
    fn test_severe_clipping() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // 5% clipped samples (severe)
        let mut samples = vec![0.5; 1000];
        for i in 0..50 {
            samples[i] = 0.99;
        }

        analyzer.process_chunk(&samples, 0, true);

        let flags = analyzer.get_flags();
        assert!(!flags.clipping_ok, "Clipping should not be OK for severe clipping");
        assert!(analyzer.total_clipped >= 50);
    }

    #[test]
    fn test_snapshot_contains_correct_values() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // Process audio with known characteristics
        let samples = vec![0.3; 512];
        analyzer.process_chunk(&samples, 0, true);
        analyzer.record_dropout();

        // Trigger snapshot at 500ms
        let snapshot = analyzer.process_chunk(&samples, 500, true);
        assert!(snapshot.is_some());

        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.timestamp_ms, 500);
        assert_eq!(snapshot.dropout_count, 1);
        assert!(snapshot.rms_db > -20.0 && snapshot.rms_db < -5.0); // ~-10.5 dBFS for 0.3 amplitude
        assert_eq!(snapshot.total_samples, 1024); // Two chunks of 512
    }

    #[test]
    fn test_overall_quality_score() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // Perfect audio: good level, no clipping, good SNR, no dropouts
        let silence = vec![0.01; 512];
        let speech = vec![0.2; 512];

        for i in 0..5 {
            analyzer.process_chunk(&silence, i as u64 * 32, false);
        }
        for i in 5..20 {
            analyzer.process_chunk(&speech, i as u64 * 32, true);
        }

        let flags = analyzer.get_flags();
        assert_eq!(flags.overall_quality, 1.0, "Perfect audio should have 1.0 quality");

        // Add dropout - should reduce quality by 25%
        analyzer.record_dropout();
        let flags = analyzer.get_flags();
        assert_eq!(flags.overall_quality, 0.75, "One issue should reduce quality to 0.75");
    }

    #[test]
    fn test_chunk_rms_calculation() {
        let analyzer = AudioQualityAnalyzer::new();

        // Known RMS: for constant value v, RMS = v
        let samples = vec![0.5; 100];
        let rms = analyzer.calculate_chunk_rms(&samples);
        assert!((rms - 0.5).abs() < 0.001);

        // Empty samples
        let empty: Vec<f32> = vec![];
        let rms = analyzer.calculate_chunk_rms(&empty);
        assert_eq!(rms, 0.0);
    }

    #[test]
    fn test_silence_ratio() {
        let mut analyzer = AudioQualityAnalyzer::new();

        // 60% silence, 40% speech
        let samples = vec![0.1; 512];

        for i in 0..6 {
            analyzer.process_chunk(&samples, i as u64 * 100, false); // silence
        }
        for i in 6..10 {
            analyzer.process_chunk(&samples, i as u64 * 100, true); // speech
        }

        // Trigger snapshot
        let snapshot = analyzer.process_chunk(&samples, 1000, true);
        assert!(snapshot.is_some());

        let snapshot = snapshot.unwrap();
        assert!((snapshot.silence_ratio - 0.6).abs() < 0.1);
    }
}
