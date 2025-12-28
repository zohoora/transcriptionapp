//! Sliding window buffer for YAMNet
//!
//! YAMNet requires 1 second (16000 samples at 16kHz) of audio.
//! We use a sliding window with 500ms hop for continuous detection.

/// Window size in samples (1 second at 16kHz)
const WINDOW_SIZE: usize = 16000;

/// Hop size in samples (500ms at 16kHz)
const HOP_SIZE: usize = 8000;

/// Sliding window buffer for continuous audio analysis
pub struct SlidingWindow {
    /// Buffer holding accumulated samples
    buffer: Vec<f32>,
    /// Position in the original stream (for timestamp calculation)
    total_samples_added: usize,
    /// Samples consumed so far
    samples_consumed: usize,
}

impl SlidingWindow {
    /// Create a new sliding window
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(WINDOW_SIZE * 2),
            total_samples_added: 0,
            samples_consumed: 0,
        }
    }

    /// Add samples to the buffer
    pub fn add_samples(&mut self, samples: &[f32]) {
        self.buffer.extend_from_slice(samples);
        self.total_samples_added += samples.len();
    }

    /// Get the next complete window if available.
    /// Returns (window_samples, start_offset_from_stream_start)
    pub fn next_window(&mut self) -> Option<(Vec<f32>, usize)> {
        if self.buffer.len() < WINDOW_SIZE {
            return None;
        }

        // Extract window
        let window: Vec<f32> = self.buffer[..WINDOW_SIZE].to_vec();
        let start_offset = self.samples_consumed;

        // Advance by hop size
        self.buffer.drain(..HOP_SIZE);
        self.samples_consumed += HOP_SIZE;

        Some((window, start_offset))
    }

    /// Check if a window is ready
    #[allow(dead_code)]
    pub fn has_window(&self) -> bool {
        self.buffer.len() >= WINDOW_SIZE
    }

    /// Clear the buffer
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.total_samples_added = 0;
        self.samples_consumed = 0;
    }
}

impl Default for SlidingWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_window_empty() {
        let mut window = SlidingWindow::new();
        assert!(window.next_window().is_none());
    }

    #[test]
    fn test_add_samples_insufficient() {
        let mut window = SlidingWindow::new();
        window.add_samples(&[0.0; 1000]);
        assert!(window.next_window().is_none());
    }

    #[test]
    fn test_first_window() {
        let mut window = SlidingWindow::new();
        window.add_samples(&[0.5; 16000]);

        let result = window.next_window();
        assert!(result.is_some());

        let (samples, offset) = result.unwrap();
        assert_eq!(samples.len(), WINDOW_SIZE);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_multiple_windows() {
        let mut window = SlidingWindow::new();

        // Add 2 seconds of audio
        window.add_samples(&[0.5; 32000]);

        // First window at offset 0
        let (_, offset1) = window.next_window().unwrap();
        assert_eq!(offset1, 0);

        // Second window at offset HOP_SIZE (8000)
        let (_, offset2) = window.next_window().unwrap();
        assert_eq!(offset2, HOP_SIZE);

        // Third window at offset 2*HOP_SIZE (16000)
        let (_, offset3) = window.next_window().unwrap();
        assert_eq!(offset3, HOP_SIZE * 2);
    }

    #[test]
    fn test_incremental_add() {
        let mut window = SlidingWindow::new();

        // Add in small chunks
        for _ in 0..32 {
            window.add_samples(&[0.5; 500]);
        }

        // Should have first window ready (16000 samples added)
        let result = window.next_window();
        assert!(result.is_some());
    }
}
