//! Extended soak tests for long-running stability testing
//!
//! These tests run for extended periods to detect:
//! - Memory leaks
//! - Performance degradation
//! - Resource exhaustion
//! - Stability issues
//!
//! Run with: SOAK_DURATION_SECS=3600 cargo test --release soak_test_extended -- --ignored --nocapture

#[cfg(test)]
mod tests {
    use crate::audio::AudioResampler;
    use crate::session::{SessionManager, SessionState};
    use crate::transcription::{Segment, Utterance};
    use crate::vad::{VadConfig, VadGatedPipeline};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    const VAD_CHUNK_SIZE: usize = 512;
    const SAMPLE_RATE: usize = 16000;

    /// Get soak test duration from environment or default to 60 seconds
    fn get_soak_duration() -> Duration {
        let secs = std::env::var("SOAK_DURATION_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        Duration::from_secs(secs)
    }

    /// Generate synthetic audio that simulates speech patterns
    fn generate_speech_like_audio(samples: usize, time_offset: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| {
                let t = (time_offset + i) as f32 / SAMPLE_RATE as f32;
                // Simulate speech: mix of frequencies with amplitude modulation
                let speech = (t * 150.0 * std::f32::consts::TAU).sin() * 0.3
                    + (t * 250.0 * std::f32::consts::TAU).sin() * 0.2
                    + (t * 400.0 * std::f32::consts::TAU).sin() * 0.1;
                // Add envelope (words have gaps)
                let envelope = ((t * 3.0).sin().abs() * 0.7 + 0.3);
                speech * envelope
            })
            .collect()
    }

    /// Generate silence with minimal noise
    fn generate_silence(samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| (i as f32 * 0.0001).sin() * 0.001)
            .collect()
    }

    /// Extended soak test for the VAD pipeline
    ///
    /// Simulates hours of audio processing to detect memory leaks
    /// and performance degradation.
    #[test]
    #[ignore] // Run explicitly with --ignored
    fn soak_test_extended_vad_pipeline() {
        let duration = get_soak_duration();
        println!("\n=== VAD Pipeline Soak Test ===");
        println!("Duration: {:?}", duration);
        println!("Starting at: {:?}", std::time::SystemTime::now());

        let config = VadConfig::from_ms(0.5, 300, 250, 500, 25000);
        let mut pipeline = VadGatedPipeline::with_config(config);

        let start = Instant::now();
        let mut total_samples: u64 = 0;
        let mut total_utterances: u64 = 0;
        let mut iterations: u64 = 0;
        let mut last_report = Instant::now();

        // Metrics for leak detection
        let initial_time = Instant::now();

        while start.elapsed() < duration {
            iterations += 1;

            // Alternate between speech and silence patterns
            let is_speech = (iterations / 100) % 2 == 0;

            if is_speech {
                // Simulate 100ms of speech
                let samples = generate_speech_like_audio(1600, total_samples as usize);
                for chunk in samples.chunks(VAD_CHUNK_SIZE) {
                    pipeline.advance_audio_clock(chunk.len());
                    total_samples += chunk.len() as u64;
                }
            } else {
                // Simulate 100ms of silence
                let samples = generate_silence(1600);
                for chunk in samples.chunks(VAD_CHUNK_SIZE) {
                    pipeline.advance_audio_clock(chunk.len());
                    total_samples += chunk.len() as u64;
                }
            }

            // Periodically force flush and collect utterances
            if iterations % 1000 == 0 {
                pipeline.force_flush();
                while let Some(_utterance) = pipeline.pop_utterance() {
                    total_utterances += 1;
                }
            }

            // Progress report every 10 seconds
            if last_report.elapsed() > Duration::from_secs(10) {
                let elapsed = start.elapsed();
                let progress = elapsed.as_secs_f64() / duration.as_secs_f64() * 100.0;
                let audio_hours = total_samples as f64 / SAMPLE_RATE as f64 / 3600.0;

                println!(
                    "[{:.1}%] Elapsed: {:?}, Iterations: {}, Audio processed: {:.2}h, Utterances: {}",
                    progress, elapsed, iterations, audio_hours, total_utterances
                );

                last_report = Instant::now();
            }
        }

        // Final flush
        pipeline.force_flush();
        while let Some(_) = pipeline.pop_utterance() {
            total_utterances += 1;
        }

        let total_elapsed = initial_time.elapsed();
        let audio_hours = total_samples as f64 / SAMPLE_RATE as f64 / 3600.0;

        println!("\n=== Soak Test Complete ===");
        println!("Total runtime: {:?}", total_elapsed);
        println!("Total iterations: {}", iterations);
        println!("Total audio processed: {:.2} hours", audio_hours);
        println!("Total utterances: {}", total_utterances);
        println!("Final audio clock: {} ms", pipeline.audio_clock_ms());
    }

    /// Extended soak test for session management
    #[test]
    #[ignore]
    fn soak_test_extended_session_management() {
        let duration = get_soak_duration();
        println!("\n=== Session Management Soak Test ===");
        println!("Duration: {:?}", duration);

        let start = Instant::now();
        let mut total_sessions: u64 = 0;
        let mut total_segments: u64 = 0;
        let mut last_report = Instant::now();

        while start.elapsed() < duration {
            // Create a new session
            let mut session = SessionManager::new();

            // Full lifecycle
            session.start_preparing().unwrap();
            session.start_recording("whisper");

            // Add varying number of segments
            let segment_count = (total_sessions % 100) as u64 + 1;
            for i in 0..segment_count {
                let segment = Segment::new(
                    i * 1000,
                    (i + 1) * 1000,
                    format!("Segment {} of session {}", i, total_sessions),
                );
                session.add_segment(segment);
                total_segments += 1;
            }

            // Get transcript (exercises string concatenation)
            let _ = session.transcript_update();

            // Complete session
            session.start_stopping().unwrap();
            session.complete();

            // Reset for next iteration
            session.reset();
            assert_eq!(session.state(), &SessionState::Idle);

            total_sessions += 1;

            // Progress report
            if last_report.elapsed() > Duration::from_secs(10) {
                let elapsed = start.elapsed();
                let progress = elapsed.as_secs_f64() / duration.as_secs_f64() * 100.0;
                println!(
                    "[{:.1}%] Sessions: {}, Segments: {}, Rate: {:.1} sessions/sec",
                    progress,
                    total_sessions,
                    total_segments,
                    total_sessions as f64 / elapsed.as_secs_f64()
                );
                last_report = Instant::now();
            }
        }

        println!("\n=== Session Soak Test Complete ===");
        println!("Total sessions: {}", total_sessions);
        println!("Total segments: {}", total_segments);
    }

    /// Extended soak test for audio resampling
    #[test]
    #[ignore]
    fn soak_test_extended_resampling() {
        let duration = get_soak_duration();
        println!("\n=== Audio Resampling Soak Test ===");
        println!("Duration: {:?}", duration);

        let mut resampler = AudioResampler::new(48000).unwrap();
        let input_frames = resampler.input_frames_next();

        let start = Instant::now();
        let mut total_input_samples: u64 = 0;
        let mut total_output_samples: u64 = 0;
        let mut iterations: u64 = 0;
        let mut last_report = Instant::now();

        // Pre-generate input buffer
        let input: Vec<f32> = (0..input_frames)
            .map(|i| ((i as f32 / 48000.0) * 440.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect();

        while start.elapsed() < duration {
            let output = resampler.process(&input).unwrap();

            total_input_samples += input.len() as u64;
            total_output_samples += output.len() as u64;
            iterations += 1;

            // Progress report
            if last_report.elapsed() > Duration::from_secs(10) {
                let elapsed = start.elapsed();
                let progress = elapsed.as_secs_f64() / duration.as_secs_f64() * 100.0;
                let input_hours = total_input_samples as f64 / 48000.0 / 3600.0;
                let output_hours = total_output_samples as f64 / 16000.0 / 3600.0;

                println!(
                    "[{:.1}%] Input: {:.2}h, Output: {:.2}h, Rate: {:.1}x realtime",
                    progress,
                    input_hours,
                    output_hours,
                    input_hours / elapsed.as_secs_f64() * 3600.0
                );
                last_report = Instant::now();
            }
        }

        let input_hours = total_input_samples as f64 / 48000.0 / 3600.0;
        println!("\n=== Resampling Soak Test Complete ===");
        println!("Total iterations: {}", iterations);
        println!("Total audio resampled: {:.2} hours", input_hours);
    }

    /// Concurrent soak test with multiple threads
    #[test]
    #[ignore]
    fn soak_test_concurrent_operations() {
        let duration = get_soak_duration();
        println!("\n=== Concurrent Operations Soak Test ===");
        println!("Duration: {:?}", duration);

        let running = Arc::new(AtomicBool::new(true));
        let total_operations = Arc::new(AtomicU64::new(0));

        let num_threads = 4;
        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let running = Arc::clone(&running);
            let total_ops = Arc::clone(&total_operations);

            let handle = thread::spawn(move || {
                let mut local_ops: u64 = 0;

                while running.load(Ordering::Relaxed) {
                    // Each thread does different work
                    match thread_id % 4 {
                        0 => {
                            // Session operations
                            let mut session = SessionManager::new();
                            session.start_preparing().unwrap();
                            session.start_recording("whisper");
                            session.add_segment(Segment::new(0, 1000, "test".to_string()));
                            session.start_stopping().unwrap();
                            session.complete();
                        }
                        1 => {
                            // VAD operations
                            let config = VadConfig::from_ms(0.5, 100, 100, 100, 10000);
                            let mut pipeline = VadGatedPipeline::with_config(config);
                            for _ in 0..100 {
                                pipeline.advance_audio_clock(512);
                            }
                        }
                        2 => {
                            // Segment creation
                            for i in 0..100 {
                                let _ = Segment::new(i, i + 1000, format!("segment {}", i));
                            }
                        }
                        3 => {
                            // Utterance creation
                            let audio: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.001).sin()).collect();
                            let _ = Utterance::new(audio, 0, 100);
                        }
                        _ => {}
                    }

                    local_ops += 1;
                    if local_ops % 1000 == 0 {
                        total_ops.fetch_add(1000, Ordering::Relaxed);
                    }
                }

                local_ops
            });

            handles.push(handle);
        }

        // Run for duration
        let start = Instant::now();
        let mut last_report = Instant::now();

        while start.elapsed() < duration {
            thread::sleep(Duration::from_secs(1));

            if last_report.elapsed() > Duration::from_secs(10) {
                let elapsed = start.elapsed();
                let progress = elapsed.as_secs_f64() / duration.as_secs_f64() * 100.0;
                let ops = total_operations.load(Ordering::Relaxed);
                println!(
                    "[{:.1}%] Total operations: {}, Rate: {:.1} ops/sec",
                    progress,
                    ops,
                    ops as f64 / elapsed.as_secs_f64()
                );
                last_report = Instant::now();
            }
        }

        // Stop all threads
        running.store(false, Ordering::Relaxed);

        let mut total_thread_ops: u64 = 0;
        for handle in handles {
            total_thread_ops += handle.join().unwrap();
        }

        println!("\n=== Concurrent Soak Test Complete ===");
        println!("Total operations: {}", total_thread_ops);
        println!("Threads: {}", num_threads);
    }

    /// Memory stress test - creates and destroys many objects
    #[test]
    #[ignore]
    fn soak_test_memory_stress() {
        let duration = get_soak_duration();
        println!("\n=== Memory Stress Soak Test ===");
        println!("Duration: {:?}", duration);

        let start = Instant::now();
        let mut iterations: u64 = 0;
        let mut last_report = Instant::now();

        while start.elapsed() < duration {
            // Create large objects and let them drop
            {
                // Large audio buffer (30 seconds at 16kHz)
                let _audio: Vec<f32> = (0..480000)
                    .map(|i| (i as f32 * 0.0001).sin())
                    .collect();

                // Many segments
                let _segments: Vec<Segment> = (0..1000)
                    .map(|i| Segment::new(i * 100, (i + 1) * 100, format!("Segment {}", i)))
                    .collect();

                // Multiple sessions
                for _ in 0..10 {
                    let mut session = SessionManager::new();
                    session.start_preparing().unwrap();
                    session.start_recording("whisper");
                    for j in 0..100 {
                        session.add_segment(Segment::new(j * 100, (j + 1) * 100, "test".to_string()));
                    }
                    session.reset();
                }
            }
            // Objects dropped here

            iterations += 1;

            if last_report.elapsed() > Duration::from_secs(10) {
                let elapsed = start.elapsed();
                let progress = elapsed.as_secs_f64() / duration.as_secs_f64() * 100.0;
                println!(
                    "[{:.1}%] Iterations: {}, Rate: {:.1} iter/sec",
                    progress,
                    iterations,
                    iterations as f64 / elapsed.as_secs_f64()
                );
                last_report = Instant::now();
            }
        }

        println!("\n=== Memory Stress Test Complete ===");
        println!("Total iterations: {}", iterations);
    }
}
