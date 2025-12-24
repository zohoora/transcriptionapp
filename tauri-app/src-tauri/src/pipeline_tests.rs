// Integration tests for the audio pipeline
// These tests use synthetic audio to verify the VAD pipeline behavior

#[cfg(test)]
mod tests {
    use crate::transcription::Utterance;
    use crate::vad::{VadConfig, VadGatedPipeline};
    use voice_activity_detector::VoiceActivityDetector;

    const VAD_CHUNK_SIZE: usize = 512;
    const SAMPLE_RATE: usize = 16000;

    // Helper to create a mock VAD that always returns the given probability
    fn create_vad() -> VoiceActivityDetector {
        VoiceActivityDetector::builder()
            .sample_rate(16000)
            .chunk_size(VAD_CHUNK_SIZE)
            .build()
            .expect("Failed to create VAD")
    }

    // Generate silence (near-zero samples)
    fn generate_silence(samples: usize) -> Vec<f32> {
        vec![0.0001; samples]
    }

    // Generate a speech-like signal (sine waves)
    fn generate_speech_signal(samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                // Mix of frequencies typical in speech
                let f1 = (2.0 * std::f32::consts::PI * 200.0 * t).sin() * 0.4;
                let f2 = (2.0 * std::f32::consts::PI * 400.0 * t).sin() * 0.3;
                let f3 = (2.0 * std::f32::consts::PI * 800.0 * t).sin() * 0.2;
                f1 + f2 + f3
            })
            .collect()
    }

    // Generate noise
    fn generate_noise(samples: usize, amplitude: f32) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        (0..samples)
            .map(|i| {
                let mut hasher = DefaultHasher::new();
                i.hash(&mut hasher);
                let hash = hasher.finish();
                ((hash as f32 / u64::MAX as f32) * 2.0 - 1.0) * amplitude
            })
            .collect()
    }

    #[test]
    fn test_pipeline_processes_silence_without_utterance() {
        let config = VadConfig::from_ms(0.5, 300, 250, 500, 25000);
        let mut pipeline = VadGatedPipeline::with_config(config);
        let mut vad = create_vad();

        // Feed 5 seconds of silence
        let silence = generate_silence(VAD_CHUNK_SIZE);
        let chunks = (5 * SAMPLE_RATE) / VAD_CHUNK_SIZE;

        for _ in 0..chunks {
            pipeline.advance_audio_clock(VAD_CHUNK_SIZE);
            pipeline.process_chunk(&silence, &mut vad);
        }

        // Should not produce any utterances
        assert!(!pipeline.has_pending_utterances());
        assert!(pipeline.pop_utterance().is_none());
    }

    #[test]
    fn test_pipeline_audio_clock_tracks_time() {
        let config = VadConfig::default();
        let mut pipeline = VadGatedPipeline::with_config(config);
        let mut vad = create_vad();

        let silence = generate_silence(VAD_CHUNK_SIZE);

        // Process approximately 1 second of audio
        let chunks_per_second = SAMPLE_RATE / VAD_CHUNK_SIZE;
        for _ in 0..chunks_per_second {
            pipeline.advance_audio_clock(VAD_CHUNK_SIZE);
            pipeline.process_chunk(&silence, &mut vad);
        }

        // Audio clock should be close to 1000ms (exact depends on chunk alignment)
        // 31 chunks * 512 samples / 16 samples per ms = 992ms
        let expected_ms = (chunks_per_second * VAD_CHUNK_SIZE) / 16;
        assert_eq!(pipeline.audio_clock_ms(), expected_ms as u64);
    }

    #[test]
    fn test_pipeline_force_flush_on_stop() {
        let config = VadConfig::from_ms(0.5, 100, 50, 500, 25000);
        let mut pipeline = VadGatedPipeline::with_config(config);

        // Manually set up speech state (simulating active speech)
        // We can't directly manipulate internal state, but we can test force_flush behavior

        // Force flush on empty pipeline should do nothing
        pipeline.force_flush();
        assert!(!pipeline.has_pending_utterances());
    }

    #[test]
    fn test_pipeline_pending_count() {
        let config = VadConfig::default();
        let pipeline = VadGatedPipeline::with_config(config);

        assert_eq!(pipeline.pending_count(), 0);
    }

    #[test]
    fn test_pipeline_is_speech_active_initially_false() {
        let pipeline = VadGatedPipeline::new();
        assert!(!pipeline.is_speech_active());
    }

    #[test]
    fn test_vad_config_sample_calculations() {
        // Verify the sample calculations are correct for 16kHz
        let config = VadConfig::from_ms(0.5, 100, 200, 300, 1000);

        // 100ms * 16 samples/ms = 1600 samples
        assert_eq!(config.pre_roll_samples, 1600);
        // 200ms * 16 = 3200
        assert_eq!(config.min_speech_samples, 3200);
        // 300ms * 16 = 4800
        assert_eq!(config.silence_to_flush_samples, 4800);
        // 1000ms * 16 = 16000
        assert_eq!(config.max_utterance_samples, 16000);
    }

    #[test]
    fn test_utterance_creation() {
        let audio = generate_speech_signal(16000); // 1 second
        let utterance = Utterance::new(audio.clone(), 0, 1000);

        assert_eq!(utterance.audio.len(), 16000);
        assert_eq!(utterance.start_ms, 0);
        assert_eq!(utterance.end_ms, 1000);
        assert_eq!(utterance.duration_ms(), 1000);
    }

    #[test]
    fn test_pipeline_multiple_sessions() {
        let config = VadConfig::default();
        let mut vad = create_vad();

        // First session
        let mut pipeline1 = VadGatedPipeline::with_config(config.clone());
        let silence = generate_silence(VAD_CHUNK_SIZE);

        for _ in 0..10 {
            pipeline1.advance_audio_clock(VAD_CHUNK_SIZE);
            pipeline1.process_chunk(&silence, &mut vad);
        }

        let clock1 = pipeline1.audio_clock_ms();

        // Second session (fresh pipeline)
        let mut pipeline2 = VadGatedPipeline::with_config(config);

        // Should start fresh
        assert_eq!(pipeline2.audio_clock_ms(), 0);
        assert!(!pipeline2.is_speech_active());

        for _ in 0..5 {
            pipeline2.advance_audio_clock(VAD_CHUNK_SIZE);
            pipeline2.process_chunk(&silence, &mut vad);
        }

        // Pipeline 1 should still have its clock
        assert_eq!(pipeline1.audio_clock_ms(), clock1);
        // Pipeline 2 should have less
        assert!(pipeline2.audio_clock_ms() < clock1);
    }

    #[test]
    fn test_continuous_audio_processing() {
        let config = VadConfig::from_ms(0.5, 300, 250, 500, 25000);
        let mut pipeline = VadGatedPipeline::with_config(config);
        let mut vad = create_vad();

        // Process 30 seconds of audio (silence)
        let silence = generate_silence(VAD_CHUNK_SIZE);
        let total_samples = 30 * SAMPLE_RATE;
        let total_chunks = total_samples / VAD_CHUNK_SIZE;

        for _ in 0..total_chunks {
            pipeline.advance_audio_clock(VAD_CHUNK_SIZE);
            pipeline.process_chunk(&silence, &mut vad);
        }

        // Should be at ~30 seconds
        let clock_ms = pipeline.audio_clock_ms();
        assert!(clock_ms >= 29900 && clock_ms <= 30100);
    }

    #[test]
    fn test_vad_with_noise() {
        let config = VadConfig::from_ms(0.5, 300, 250, 500, 25000);
        let mut pipeline = VadGatedPipeline::with_config(config);
        let mut vad = create_vad();

        // Process low-amplitude noise (shouldn't trigger VAD)
        let noise = generate_noise(VAD_CHUNK_SIZE, 0.01);

        for _ in 0..100 {
            pipeline.advance_audio_clock(VAD_CHUNK_SIZE);
            pipeline.process_chunk(&noise, &mut vad);
        }

        // Low noise shouldn't be detected as speech
        // (though this depends on VAD sensitivity)
        assert!(!pipeline.has_pending_utterances() || pipeline.pending_count() == 0);
    }
}
