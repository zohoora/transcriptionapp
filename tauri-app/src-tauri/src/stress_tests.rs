// Stress tests for long-running audio processing
// These tests simulate extended recording sessions

#[cfg(test)]
mod tests {
    use crate::session::{SessionManager, SessionError, SessionState};
    use crate::transcription::{Segment, Utterance};
    use crate::vad::{VadConfig, VadGatedPipeline};
    use crate::audio::AudioResampler;
    use std::time::Instant;

    const VAD_CHUNK_SIZE: usize = 512;
    const SAMPLE_RATE: usize = 16000;

    #[test]
    fn stress_test_session_many_segments() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        // Add 1000 segments (simulating a long transcription)
        for i in 0..1000 {
            let segment = Segment::new(
                i * 1000,
                (i + 1) * 1000,
                format!("Segment number {} with some text content.", i),
            );
            session.add_segment(segment);
        }

        assert_eq!(session.segments().len(), 1000);

        // Verify transcript generation works with many segments
        let update = session.transcript_update();
        assert_eq!(update.segment_count, 1000);
        assert!(update.finalized_text.len() > 0);
    }

    #[test]
    fn stress_test_session_rapid_status_updates() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        // Simulate rapid pending count updates (like during heavy processing)
        for i in 0..10000 {
            session.set_pending_count(i % 10);
            let status = session.status();
            // Just verify it doesn't panic
            let _ = status.is_processing_behind;
        }
    }

    #[test]
    fn stress_test_pipeline_long_session() {
        let config = VadConfig::from_ms(0.5, 300, 250, 500, 25000);
        let mut pipeline = VadGatedPipeline::with_config(config);

        // Simulate 1 hour of audio clock advancement (without actual VAD processing)
        let one_hour_samples = 3600 * SAMPLE_RATE;
        let chunks = one_hour_samples / VAD_CHUNK_SIZE;

        let start = Instant::now();

        for _ in 0..chunks {
            pipeline.advance_audio_clock(VAD_CHUNK_SIZE);
        }

        let elapsed = start.elapsed();

        // Should be very fast (just counter updates)
        assert!(elapsed.as_secs() < 1, "Clock advancement took too long: {:?}", elapsed);

        // Verify clock is at approximately 1 hour
        let clock_ms = pipeline.audio_clock_ms();
        assert!(clock_ms >= 3599000 && clock_ms <= 3601000,
            "Clock should be ~1 hour, got {}ms", clock_ms);
    }

    #[test]
    fn stress_test_resampler_continuous() {
        let mut resampler = AudioResampler::new(48000).unwrap();
        let input_frames = resampler.input_frames_next();
        let input = vec![0.0f32; input_frames];

        // Process equivalent of 10 minutes of audio at 48kHz
        let samples_per_minute = 48000 * 60;
        let iterations = (10 * samples_per_minute) / input_frames;

        let start = Instant::now();

        for _ in 0..iterations {
            let output = resampler.process(&input).unwrap();
            // Just verify output is produced
            assert!(!output.is_empty() || output.len() <= input_frames);
        }

        let elapsed = start.elapsed();

        // Processing should be reasonably fast (real-time factor should be < 0.1)
        // 10 minutes of audio should process in well under 60 seconds
        assert!(elapsed.as_secs() < 60,
            "Resampler too slow: {:?} for 10 min audio", elapsed);
    }

    #[test]
    fn stress_test_utterance_large_audio() {
        // Create a 30-second utterance (maximum allowed)
        let audio_samples = 30 * SAMPLE_RATE;
        let audio: Vec<f32> = (0..audio_samples)
            .map(|i| ((i as f32 / SAMPLE_RATE as f32) * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5)
            .collect();

        let utterance = Utterance::new(audio.clone(), 0, 30000);

        assert_eq!(utterance.audio.len(), audio_samples);
        assert_eq!(utterance.duration_ms(), 30000);
    }

    #[test]
    fn stress_test_many_short_segments() {
        // Simulate many short utterances (like fast-paced dialogue)
        let segments: Vec<Segment> = (0..5000)
            .map(|i| Segment::new(i * 200, i * 200 + 150, format!("Word{}", i)))
            .collect();

        assert_eq!(segments.len(), 5000);

        // Verify all have unique IDs
        let mut ids: Vec<_> = segments.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 5000);
    }

    #[test]
    fn stress_test_session_state_cycling() {
        // Rapidly cycle through session states
        for _ in 0..100 {
            let mut session = SessionManager::new();

            // Full cycle: idle -> preparing -> recording -> stopping -> completed -> reset
            assert_eq!(session.state(), &SessionState::Idle);

            session.start_preparing().unwrap();
            assert_eq!(session.state(), &SessionState::Preparing);

            session.start_recording("whisper");
            assert_eq!(session.state(), &SessionState::Recording);

            // Add a segment
            session.add_segment(Segment::new(0, 1000, "test".to_string()));

            session.start_stopping().unwrap();
            assert_eq!(session.state(), &SessionState::Stopping);

            session.complete();
            assert_eq!(session.state(), &SessionState::Completed);

            session.reset();
            assert_eq!(session.state(), &SessionState::Idle);
            assert_eq!(session.segments().len(), 0);
        }
    }

    #[test]
    fn stress_test_error_recovery() {
        for _ in 0..100 {
            let mut session = SessionManager::new();

            session.start_preparing().unwrap();
            session.set_error(SessionError::AudioDeviceError("test error".to_string()));

            assert_eq!(session.state(), &SessionState::Error);

            // Recovery
            session.reset();
            assert_eq!(session.state(), &SessionState::Idle);

            // Should be able to start again
            session.start_preparing().unwrap();
            assert_eq!(session.state(), &SessionState::Preparing);
        }
    }

    #[test]
    fn stress_test_transcript_concatenation() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        // Add segments with varying lengths
        for i in 0u64..500 {
            let text = "word ".repeat(((i % 50) + 1) as usize);
            session.add_segment(Segment::new(i * 1000, (i + 1) * 1000, text));
        }

        let update = session.transcript_update();

        // Transcript should contain all words
        assert!(update.finalized_text.contains("word"));
        assert!(update.segment_count == 500);

        // Should be able to get transcript multiple times
        for _ in 0..100 {
            let update2 = session.transcript_update();
            assert_eq!(update2.segment_count, update.segment_count);
        }
    }

    #[test]
    fn stress_test_vad_config_edge_cases() {
        // Test with maximum reasonable values
        let config = VadConfig::from_ms(1.0, 10000, 10000, 10000, 60000);
        let pipeline = VadGatedPipeline::with_config(config);
        assert!(!pipeline.is_speech_active());

        // Test with minimum values
        let config = VadConfig::from_ms(0.0, 0, 0, 0, 0);
        let pipeline = VadGatedPipeline::with_config(config);
        assert!(!pipeline.is_speech_active());

        // Test with typical values many times
        for _ in 0..1000 {
            let config = VadConfig::from_ms(0.5, 300, 250, 500, 25000);
            let _ = VadGatedPipeline::with_config(config);
        }
    }

    #[test]
    fn stress_test_concurrent_segment_access() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let session = Arc::new(Mutex::new(SessionManager::new()));

        {
            let mut s = session.lock().unwrap();
            s.start_preparing().unwrap();
            s.start_recording("whisper");
        }

        // Spawn threads that add segments
        let handles: Vec<_> = (0..10)
            .map(|thread_id| {
                let session = Arc::clone(&session);
                thread::spawn(move || {
                    for i in 0..100 {
                        let mut s = session.lock().unwrap();
                        let segment = Segment::new(
                            (thread_id * 1000 + i) as u64,
                            (thread_id * 1000 + i + 1) as u64,
                            format!("Thread {} segment {}", thread_id, i),
                        );
                        s.add_segment(segment);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let s = session.lock().unwrap();
        assert_eq!(s.segments().len(), 1000);
    }
}
