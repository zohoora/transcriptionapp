//! Thermal frame analysis for presence detection and occupancy counting.
//!
//! Processes 32x24 MLX90640 thermal camera frames using:
//! - Hot-pixel thresholding to detect human body heat
//! - Connected-component labeling (flood-fill) to count distinct people
//!
//! All functions are pure — no state, no side effects. Easy to test.

use super::types::ThermalConfig;

/// Count pixels above the temperature threshold.
pub fn count_hot_pixels(frame: &[f32], threshold_c: f32) -> usize {
    frame.iter().filter(|&&t| t >= threshold_c).count()
}

/// Binary presence detection from a thermal frame.
///
/// Returns true if enough hot pixels are detected to indicate a person.
/// Uses `min_blob_pixels` as the minimum threshold (a single person blob).
pub fn thermal_presence(frame: &[f32], config: &ThermalConfig) -> bool {
    let hot = count_hot_pixels(frame, config.hot_pixel_threshold_c);
    hot >= config.min_blob_pixels
}

/// Estimate the number of people in a thermal frame using connected-component labeling.
///
/// Algorithm: flood-fill on the 2D grid to find connected blobs of hot pixels.
/// Each blob with >= `min_blob_pixels` pixels is counted as one person.
///
/// Works on the 32x24 MLX90640 grid (768 cells — trivially fast).
pub fn estimate_occupancy(frame: &[f32], w: u16, h: u16, config: &ThermalConfig) -> u8 {
    let w = w as usize;
    let h = h as usize;

    if frame.len() != w * h {
        return 0;
    }

    // Build binary mask: true = hot pixel
    let mask: Vec<bool> = frame
        .iter()
        .map(|&t| t >= config.hot_pixel_threshold_c)
        .collect();

    // Connected-component labeling via flood-fill
    let mut visited = vec![false; w * h];
    let mut blob_count: u8 = 0;

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if mask[idx] && !visited[idx] {
                // Flood-fill to find connected blob
                let blob_size = flood_fill(&mask, &mut visited, x, y, w, h);
                if blob_size >= config.min_blob_pixels {
                    blob_count = blob_count.saturating_add(1);
                }
            }
        }
    }

    blob_count
}

/// Flood-fill from (start_x, start_y) to count connected hot pixels.
/// Uses 4-connectivity (up/down/left/right).
fn flood_fill(
    mask: &[bool],
    visited: &mut [bool],
    start_x: usize,
    start_y: usize,
    w: usize,
    h: usize,
) -> usize {
    let mut stack = vec![(start_x, start_y)];
    let mut count = 0;

    while let Some((x, y)) = stack.pop() {
        let idx = y * w + x;
        if visited[idx] || !mask[idx] {
            continue;
        }
        visited[idx] = true;
        count += 1;

        // 4-connected neighbors
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> ThermalConfig {
        ThermalConfig {
            hot_pixel_threshold_c: 28.0,
            min_blob_pixels: 4,
        }
    }

    #[test]
    fn test_count_hot_pixels_empty() {
        assert_eq!(count_hot_pixels(&[], 28.0), 0);
    }

    #[test]
    fn test_count_hot_pixels_none_hot() {
        let frame = vec![20.0; 768];
        assert_eq!(count_hot_pixels(&frame, 28.0), 0);
    }

    #[test]
    fn test_count_hot_pixels_all_hot() {
        let frame = vec![32.0; 768];
        assert_eq!(count_hot_pixels(&frame, 28.0), 768);
    }

    #[test]
    fn test_count_hot_pixels_mixed() {
        let frame = vec![20.0, 30.0, 25.0, 35.0, 28.0];
        assert_eq!(count_hot_pixels(&frame, 28.0), 3); // 30, 35, 28
    }

    #[test]
    fn test_thermal_presence_empty_room() {
        let frame = vec![22.0; 768]; // All ambient
        assert!(!thermal_presence(&frame, &default_config()));
    }

    #[test]
    fn test_thermal_presence_one_person() {
        let mut frame = vec![22.0; 768];
        // Place a 4-pixel hot spot (body heat)
        frame[0] = 32.0;
        frame[1] = 31.0;
        frame[32] = 33.0; // Below first pixel (width=32)
        frame[33] = 30.0;
        assert!(thermal_presence(&frame, &default_config()));
    }

    #[test]
    fn test_thermal_presence_below_threshold() {
        let mut frame = vec![22.0; 768];
        // Only 3 hot pixels — below min_blob_pixels=4
        frame[0] = 32.0;
        frame[1] = 31.0;
        frame[2] = 30.0;
        assert!(!thermal_presence(&frame, &default_config()));
    }

    #[test]
    fn test_estimate_occupancy_empty() {
        let frame = vec![22.0; 768];
        assert_eq!(estimate_occupancy(&frame, 32, 24, &default_config()), 0);
    }

    #[test]
    fn test_estimate_occupancy_one_person() {
        let mut frame = vec![22.0; 768];
        // One connected blob of 6 pixels
        frame[0] = 32.0;
        frame[1] = 31.0;
        frame[2] = 30.0;
        frame[32] = 33.0;
        frame[33] = 31.0;
        frame[34] = 30.0;
        assert_eq!(estimate_occupancy(&frame, 32, 24, &default_config()), 1);
    }

    #[test]
    fn test_estimate_occupancy_two_people() {
        let mut frame = vec![22.0; 768];
        // Blob 1: top-left (connected 2x3)
        frame[0] = 32.0;
        frame[1] = 31.0;
        frame[32] = 33.0;
        frame[33] = 31.0;
        // Blob 2: far away (connected 2x3)
        frame[20] = 32.0;
        frame[21] = 31.0;
        frame[52] = 33.0; // 20 + 32
        frame[53] = 31.0;
        assert_eq!(estimate_occupancy(&frame, 32, 24, &default_config()), 2);
    }

    #[test]
    fn test_estimate_occupancy_small_blobs_ignored() {
        let mut frame = vec![22.0; 768];
        // Two tiny blobs of 2 pixels each (below min_blob_pixels=4)
        frame[0] = 32.0;
        frame[1] = 31.0;
        frame[100] = 33.0;
        frame[101] = 30.0;
        assert_eq!(estimate_occupancy(&frame, 32, 24, &default_config()), 0);
    }

    #[test]
    fn test_estimate_occupancy_wrong_frame_size() {
        let frame = vec![22.0; 100]; // Wrong size
        assert_eq!(estimate_occupancy(&frame, 32, 24, &default_config()), 0);
    }

    #[test]
    fn test_flood_fill_l_shape() {
        // L-shaped blob should be counted as one connected component
        let mut frame = vec![22.0; 768];
        // Vertical bar: (0,0), (0,1), (0,2)
        frame[0] = 32.0;
        frame[32] = 32.0;
        frame[64] = 32.0;
        // Horizontal extension: (1,2), (2,2)
        frame[65] = 32.0;
        frame[66] = 32.0;
        // 5 connected pixels = one person
        assert_eq!(estimate_occupancy(&frame, 32, 24, &default_config()), 1);
    }

    #[test]
    fn test_diagonal_pixels_not_connected() {
        // Diagonal pixels should NOT be connected (4-connectivity)
        let mut frame = vec![22.0; 768];
        frame[0] = 32.0;    // (0,0)
        frame[33] = 32.0;   // (1,1)
        frame[66] = 32.0;   // (2,2)
        frame[99] = 32.0;   // (3,3)
        // 4 pixels but all diagonal — none connected → 0 blobs above min_blob_pixels
        assert_eq!(estimate_occupancy(&frame, 32, 24, &default_config()), 0);
    }
}
