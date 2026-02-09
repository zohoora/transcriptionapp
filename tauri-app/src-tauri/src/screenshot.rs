//! Screen capture module for periodic screenshot capture during recording sessions.
//!
//! Uses macOS CoreGraphics to capture the full display and saves as JPEG
//! to a temporary directory. Screenshots are cleaned up when capture stops.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, warn};

/// State for the screen capture system
#[derive(Debug)]
pub struct ScreenCaptureState {
    /// Whether capture is currently running
    running: Arc<AtomicBool>,
    /// Paths of captured screenshots
    screenshots: Arc<Mutex<Vec<PathBuf>>>,
    /// Temp directory for this capture session
    temp_dir: Option<PathBuf>,
    /// Join handle for the capture thread
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl Default for ScreenCaptureState {
    fn default() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            screenshots: Arc::new(Mutex::new(Vec::new())),
            temp_dir: None,
            thread_handle: None,
        }
    }
}

impl ScreenCaptureState {
    /// Start periodic screen capture at the given interval.
    /// Returns Ok(()) if capture started, Err if already running or setup failed.
    pub fn start(&mut self, interval_secs: u32) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("Screen capture already running".to_string());
        }

        // Join any previous thread handle to avoid leaking resources
        if let Some(old_handle) = self.thread_handle.take() {
            let _ = old_handle.join();
        }

        // Clean up previous session's temp directory before creating a new one
        self.cleanup_temp_files();

        // Create temp directory for this session
        let temp_dir = std::env::temp_dir().join(format!(
            "transcriptionapp-screenshots-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        ));
        std::fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

        info!("Starting screen capture to {:?}, interval {}s", temp_dir, interval_secs);

        self.temp_dir = Some(temp_dir.clone());
        self.running.store(true, Ordering::SeqCst);

        // Clear previous screenshot paths
        if let Ok(mut ss) = self.screenshots.lock() {
            ss.clear();
        }

        let running = self.running.clone();
        let screenshots = self.screenshots.clone();
        let interval = Duration::from_secs(interval_secs as u64);

        let handle = std::thread::Builder::new()
            .name("screen-capture".to_string())
            .spawn(move || {
                // Capture immediately on start
                capture_and_save(&temp_dir, &screenshots);

                while running.load(Ordering::SeqCst) {
                    // Sleep in small increments so we can stop quickly
                    let mut elapsed = Duration::ZERO;
                    let tick = Duration::from_millis(250);
                    while elapsed < interval && running.load(Ordering::SeqCst) {
                        std::thread::sleep(tick);
                        elapsed += tick;
                    }

                    if running.load(Ordering::SeqCst) {
                        capture_and_save(&temp_dir, &screenshots);
                    }
                }

                info!("Screen capture thread stopped");
            })
            .map_err(|e| format!("Failed to spawn capture thread: {}", e))?;

        self.thread_handle = Some(handle);
        Ok(())
    }

    /// Stop capture. Screenshot files are retained for vision SOAP until
    /// the next `start()` call or explicit `cleanup_temp_files()`.
    pub fn stop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        info!("Stopping screen capture");
        self.running.store(false, Ordering::SeqCst);

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        if let Some(ref dir) = self.temp_dir {
            let count = self.screenshots.lock().map(|ss| ss.len()).unwrap_or(0);
            info!("Screen capture stopped. {} screenshots retained in {:?}", count, dir);
        }
    }

    /// Remove the temp directory and all screenshot files from this session.
    /// Called automatically on the next `start()` and on `Drop`.
    pub fn cleanup_temp_files(&mut self) {
        if let Some(dir) = self.temp_dir.take() {
            if dir.exists() {
                match std::fs::remove_dir_all(&dir) {
                    Ok(()) => info!("Cleaned up screenshot temp dir: {:?}", dir),
                    Err(e) => warn!("Failed to clean up screenshot temp dir {:?}: {}", dir, e),
                }
            }
        }
        if let Ok(mut ss) = self.screenshots.lock() {
            ss.clear();
        }
    }

    /// Check if capture is currently running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get paths of all captured screenshots
    pub fn screenshot_paths(&self) -> Vec<PathBuf> {
        self.screenshots.lock().map(|ss| ss.clone()).unwrap_or_default()
    }

    /// Get the number of captured screenshots
    pub fn screenshot_count(&self) -> usize {
        self.screenshots.lock().map(|ss| ss.len()).unwrap_or(0)
    }
}

impl Drop for ScreenCaptureState {
    fn drop(&mut self) {
        self.stop();
        self.cleanup_temp_files();
    }
}

/// Capture a screenshot and save both full-size and resized versions to the temp directory
fn capture_and_save(temp_dir: &Path, screenshots: &Arc<Mutex<Vec<PathBuf>>>) {
    debug!("Attempting screen capture...");
    match capture_screen() {
        Ok(image_data) => {
            let timestamp = chrono::Utc::now().format("%H%M%S-%3f");

            // Save full-size version
            let full_path = temp_dir.join(format!("capture-{}-full.jpg", timestamp));
            match save_jpeg(&image_data, &full_path) {
                Ok(()) => {
                    debug!("Full screenshot saved: {:?} ({}x{}, {} bytes)",
                           full_path, image_data.width, image_data.height,
                           std::fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0));
                    if let Ok(mut ss) = screenshots.lock() {
                        ss.push(full_path);
                    }
                }
                Err(e) => warn!("Failed to save full screenshot: {}", e),
            }

            // Save resized version (~1150px on the long edge)
            let thumb_path = temp_dir.join(format!("capture-{}-thumb.jpg", timestamp));
            match save_jpeg_resized(&image_data, &thumb_path, 1150) {
                Ok((tw, th)) => {
                    debug!("Resized screenshot saved: {:?} ({}x{}, {} bytes)",
                           thumb_path, tw, th,
                           std::fs::metadata(&thumb_path).map(|m| m.len()).unwrap_or(0));
                    if let Ok(mut ss) = screenshots.lock() {
                        ss.push(thumb_path);
                    }
                }
                Err(e) => warn!("Failed to save resized screenshot: {}", e),
            }
        }
        Err(e) => warn!("Failed to capture screen: {}", e),
    }
}

/// Raw image data from screen capture
struct RawImage {
    width: u32,
    height: u32,
    /// RGBA pixel data
    data: Vec<u8>,
}

/// Capture the window under the mouse cursor using CoreGraphics (macOS).
/// Falls back to full screen capture if no window is found.
#[cfg(target_os = "macos")]
fn capture_screen() -> Result<RawImage, String> {
    use core_graphics::display::{CGDisplay, CGPoint, CGRect, CGSize};

    // Try to find and capture the window under the cursor
    match find_window_under_cursor() {
        Some((window_id, window_name)) => {
            debug!("Capturing window under cursor: '{}' (id: {})", window_name, window_id);

            // CGRectNull (all zeros) = use window's own bounds
            let null_bounds = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize { width: 0.0, height: 0.0 },
            };

            // CGDisplay::screenshot wraps CGWindowListCreateImage and handles CGImage construction.
            // list_option=8 = kCGWindowListOptionIncludingWindow
            // image_option=1 = kCGWindowImageBoundsIgnoreFraming
            match CGDisplay::screenshot(null_bounds, 8, window_id, 1) {
                Some(image) => cgimage_to_raw(&image),
                None => {
                    warn!("Window capture returned null for '{}' (id: {}), falling back to full screen", window_name, window_id);
                    capture_full_screen()
                }
            }
        }
        None => {
            debug!("No window found under cursor, capturing full screen");
            capture_full_screen()
        }
    }
}

/// Find the window under the current mouse cursor position.
/// Returns (window_id, window_owner_name) or None.
#[cfg(target_os = "macos")]
fn find_window_under_cursor() -> Option<(u32, String)> {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    // Get current mouse position
    let mouse_event = unsafe { CGEventCreate(std::ptr::null()) };
    if mouse_event.is_null() {
        return None;
    }
    let mouse_loc = unsafe { CGEventGetLocation(mouse_event) };
    unsafe { CFRelease(mouse_event as *const _) };

    let cursor_x = mouse_loc.x;
    let cursor_y = mouse_loc.y;

    // Get on-screen window list (front-to-back order)
    // kCGWindowListOptionOnScreenOnly = 1 << 0 = 1
    // kCGWindowListExcludeDesktopElements = 1 << 4 = 16
    let window_list = unsafe {
        CGWindowListCopyWindowInfo(1 | 16, 0) // onScreenOnly | excludeDesktop, kCGNullWindowID
    };
    if window_list.is_null() {
        return None;
    }

    let windows: CFArray<CFDictionary<CFString, CFType>> = unsafe { TCFType::wrap_under_create_rule(window_list) };

    let key_bounds = CFString::new("kCGWindowBounds");
    let key_number = CFString::new("kCGWindowNumber");
    let key_owner = CFString::new("kCGWindowOwnerName");
    let key_layer = CFString::new("kCGWindowLayer");
    let key_x = CFString::new("X");
    let key_y = CFString::new("Y");
    let key_w = CFString::new("Width");
    let key_h = CFString::new("Height");

    let self_name = "Transcription App";

    for i in 0..windows.len() {
        let dict = unsafe { windows.get_unchecked(i) };

        // Skip windows not on the normal layer (layer 0)
        if let Some(layer_val) = dict.find(&key_layer) {
            let layer_ref: CFNumber = unsafe { TCFType::wrap_under_get_rule(layer_val.as_CFTypeRef() as *const _) };
            if let Some(layer) = layer_ref.to_i32() {
                if layer != 0 {
                    continue;
                }
            }
        }

        // Skip our own app's windows
        if let Some(owner_val) = dict.find(&key_owner) {
            let owner: CFString = unsafe { TCFType::wrap_under_get_rule(owner_val.as_CFTypeRef() as *const _) };
            let owner_str = owner.to_string();
            if owner_str == self_name {
                continue;
            }

            // Get window bounds
            if let Some(bounds_val) = dict.find(&key_bounds) {
                let bounds: CFDictionary<CFString, CFType> = unsafe { TCFType::wrap_under_get_rule(bounds_val.as_CFTypeRef() as *const _) };

                let x = get_cf_number(&bounds, &key_x).unwrap_or(0.0);
                let y = get_cf_number(&bounds, &key_y).unwrap_or(0.0);
                let w = get_cf_number(&bounds, &key_w).unwrap_or(0.0);
                let h = get_cf_number(&bounds, &key_h).unwrap_or(0.0);

                // Check if cursor is inside this window
                if cursor_x >= x && cursor_x <= x + w && cursor_y >= y && cursor_y <= y + h {
                    // Get window ID
                    if let Some(id_val) = dict.find(&key_number) {
                        let id_num: CFNumber = unsafe { TCFType::wrap_under_get_rule(id_val.as_CFTypeRef() as *const _) };
                        if let Some(window_id) = id_num.to_i32() {
                            return Some((window_id as u32, owner_str));
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn get_cf_number(dict: &core_foundation::dictionary::CFDictionary<core_foundation::string::CFString, core_foundation::base::CFType>, key: &core_foundation::string::CFString) -> Option<f64> {
    use core_foundation::base::TCFType;
    use core_foundation::number::CFNumber;

    dict.find(key).and_then(|val| {
        let num: CFNumber = unsafe { TCFType::wrap_under_get_rule(val.as_CFTypeRef() as *const _) };
        num.to_f64()
    })
}

/// Full screen capture fallback
#[cfg(target_os = "macos")]
fn capture_full_screen() -> Result<RawImage, String> {
    use core_graphics::display::{CGDisplay, CGPoint, CGRect, CGSize};

    let display = CGDisplay::main();
    let bounds = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: display.pixels_wide() as f64,
            height: display.pixels_high() as f64,
        },
    };

    let image = CGDisplay::screenshot(bounds, 0, 0, 0)
        .ok_or_else(|| "CGDisplayCreateImage returned null — screen recording permission may not be granted".to_string())?;

    cgimage_to_raw(&image)
}

/// Convert a CGImage to RawImage (RGBA pixel data)
#[cfg(target_os = "macos")]
fn cgimage_to_raw(image: &core_graphics::image::CGImage) -> Result<RawImage, String> {
    let width = image.width() as u32;
    let height = image.height() as u32;
    let bytes_per_row = image.bytes_per_row();
    let raw_data = image.data();
    let bytes = raw_data.bytes();

    // CoreGraphics returns BGRA (premultiplied alpha, first) for display captures.
    // Convert BGRA → RGBA for the image crate.
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        let row_start = y * bytes_per_row;
        for x in 0..width as usize {
            let offset = row_start + x * 4;
            if offset + 3 < bytes.len() {
                rgba.push(bytes[offset + 2]); // R (from B position in BGRA)
                rgba.push(bytes[offset + 1]); // G
                rgba.push(bytes[offset]);     // B (from R position in BGRA)
                rgba.push(255);               // A (opaque)
            }
        }
    }

    Ok(RawImage { width, height, data: rgba })
}

// CoreGraphics FFI for window enumeration and cursor position
#[cfg(target_os = "macos")]
extern "C" {
    fn CGWindowListCopyWindowInfo(
        option: u32,
        relative_to_window: u32,
    ) -> core_foundation::array::CFArrayRef;

    fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *const std::ffi::c_void) -> core_graphics::display::CGPoint;
    fn CFRelease(cf: *const std::ffi::c_void);
}

/// Fallback for non-macOS — screen capture not supported
#[cfg(not(target_os = "macos"))]
fn capture_screen() -> Result<RawImage, String> {
    Err("Screen capture is only supported on macOS".to_string())
}

/// Save raw RGBA image data as JPEG
fn save_jpeg(raw: &RawImage, path: &Path) -> Result<(), String> {
    use image::{ImageBuffer, Rgba, codecs::jpeg::JpegEncoder};
    use std::fs::File;
    use std::io::BufWriter;

    let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(raw.width, raw.height, raw.data.clone())
        .ok_or("Failed to create image buffer")?;

    let file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;
    let writer = BufWriter::new(file);

    let mut encoder = JpegEncoder::new_with_quality(writer, 70);
    encoder.encode_image(&img).map_err(|e| format!("JPEG encode failed: {}", e))?;

    Ok(())
}

/// Save raw RGBA image data as JPEG, resized so the long edge is `max_edge` pixels.
/// Returns the (width, height) of the resized image.
fn save_jpeg_resized(raw: &RawImage, path: &Path, max_edge: u32) -> Result<(u32, u32), String> {
    use image::{ImageBuffer, Rgba, codecs::jpeg::JpegEncoder, imageops::FilterType};
    use std::fs::File;
    use std::io::BufWriter;

    let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(raw.width, raw.height, raw.data.clone())
        .ok_or("Failed to create image buffer")?;

    let long_edge = raw.width.max(raw.height);
    let (new_w, new_h) = if long_edge <= max_edge {
        (raw.width, raw.height)
    } else {
        let scale = max_edge as f64 / long_edge as f64;
        ((raw.width as f64 * scale).round() as u32, (raw.height as f64 * scale).round() as u32)
    };

    let resized = image::imageops::resize(&img, new_w, new_h, FilterType::Lanczos3);

    let file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;
    let writer = BufWriter::new(file);

    let mut encoder = JpegEncoder::new_with_quality(writer, 70);
    encoder.encode_image(&resized).map_err(|e| format!("JPEG encode failed: {}", e))?;

    Ok((new_w, new_h))
}

/// Capture the screen and return a resized JPEG as a base64-encoded string.
///
/// Used by continuous mode for vision-based patient name extraction.
/// No temp files are written — the image stays in memory.
pub fn capture_to_base64(max_edge: u32) -> Result<String, String> {
    use base64::Engine;
    use image::{ImageBuffer, Rgba, codecs::jpeg::JpegEncoder, imageops::FilterType};
    use std::io::Cursor;

    let raw = capture_screen()?;

    let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(raw.width, raw.height, raw.data.clone())
        .ok_or("Failed to create image buffer")?;

    // Resize to fit within max_edge
    let long_edge = raw.width.max(raw.height);
    let (new_w, new_h) = if long_edge <= max_edge {
        (raw.width, raw.height)
    } else {
        let scale = max_edge as f64 / long_edge as f64;
        (
            (raw.width as f64 * scale).round() as u32,
            (raw.height as f64 * scale).round() as u32,
        )
    };

    let resized = image::imageops::resize(&img, new_w, new_h, FilterType::Lanczos3);

    // Encode as JPEG to in-memory buffer
    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut buf), 70);
    encoder
        .encode_image(&resized)
        .map_err(|e| format!("JPEG encode failed: {}", e))?;

    debug!(
        "Screen capture to base64: {}x{} → {}x{}, {} bytes JPEG",
        raw.width, raw.height, new_w, new_h, buf.len()
    );

    Ok(base64::engine::general_purpose::STANDARD.encode(&buf))
}

/// Select evenly-spaced thumbnail screenshots from the captured list.
///
/// Filters for `-thumb.jpg` files and picks up to `count` images
/// evenly distributed through the timeline.
pub fn select_thumbnails(screenshots: &[PathBuf], count: usize) -> Vec<PathBuf> {
    let thumbs: Vec<&PathBuf> = screenshots
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.contains("-thumb.jpg"))
                .unwrap_or(false)
        })
        .collect();

    if thumbs.is_empty() || count == 0 {
        return Vec::new();
    }

    let count = count.min(3).min(thumbs.len());

    if count >= thumbs.len() {
        return thumbs.into_iter().cloned().collect();
    }

    // Pick evenly spaced indices
    let mut selected = Vec::with_capacity(count);
    for i in 0..count {
        let idx = i * (thumbs.len() - 1) / (count - 1).max(1);
        selected.push(thumbs[idx].clone());
    }
    selected
}

/// Stitch multiple thumbnail images horizontally into a single composite JPEG.
///
/// Returns the base64-encoded JPEG string. Uses quality 70 to keep size reasonable.
/// The composite is created by placing images side-by-side horizontally.
pub fn stitch_thumbnails_to_base64(paths: &[PathBuf]) -> Result<String, String> {
    use base64::Engine;
    use image::{DynamicImage, RgbaImage, codecs::jpeg::JpegEncoder};
    use std::io::Cursor;

    if paths.is_empty() {
        return Err("No thumbnail paths provided".to_string());
    }

    // Load all images
    let images: Vec<DynamicImage> = paths
        .iter()
        .map(|p| {
            image::open(p).map_err(|e| format!("Failed to open {:?}: {}", p, e))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if images.len() == 1 {
        // Single image: just encode directly
        let img = &images[0];
        let mut buf = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut buf), 70);
        encoder.encode_image(img).map_err(|e| format!("JPEG encode failed: {}", e))?;
        return Ok(base64::engine::general_purpose::STANDARD.encode(&buf));
    }

    // Calculate composite dimensions (horizontal layout)
    let total_width: u32 = images.iter().map(|img| img.width()).sum();
    let max_height: u32 = images.iter().map(|img| img.height()).max().unwrap_or(0);

    // Create composite canvas
    let mut composite = RgbaImage::new(total_width, max_height);

    // Place images side-by-side
    let mut x_offset = 0u32;
    for img in &images {
        let rgba = img.to_rgba8();
        for (x, y, pixel) in rgba.enumerate_pixels() {
            if x + x_offset < total_width && y < max_height {
                composite.put_pixel(x + x_offset, y, *pixel);
            }
        }
        x_offset += img.width();
    }

    // Encode as JPEG
    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut buf), 70);
    encoder
        .encode_image(&DynamicImage::ImageRgba8(composite))
        .map_err(|e| format!("JPEG encode failed: {}", e))?;

    info!(
        "Stitched {} thumbnails into {}x{} composite ({} bytes)",
        images.len(),
        total_width,
        max_height,
        buf.len()
    );

    Ok(base64::engine::general_purpose::STANDARD.encode(&buf))
}

/// Check if screen recording permission is granted (macOS).
/// Attempts a minimal capture — if it returns null, permission is not granted.
#[cfg(target_os = "macos")]
pub fn check_screen_recording_permission() -> bool {
    use core_graphics::display::{CGDisplay, CGPoint, CGRect, CGSize};

    // Capture a 1x1 pixel area — if permission is denied, this returns None
    let bounds = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: 1.0, height: 1.0 },
    };

    CGDisplay::screenshot(bounds, 0, 0, 0).is_some()
}

#[cfg(not(target_os = "macos"))]
pub fn check_screen_recording_permission() -> bool {
    true
}

/// Open System Settings to Screen Recording privacy section (macOS)
#[cfg(target_os = "macos")]
pub fn open_screen_recording_settings() -> Result<(), String> {
    use std::process::Command;

    info!("Opening Screen Recording privacy settings...");
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn()
        .map_err(|e| format!("Failed to open settings: {}", e))?;

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn open_screen_recording_settings() -> Result<(), String> {
    Err("Screen recording settings not available on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let state = ScreenCaptureState::default();
        assert!(!state.is_running());
        assert_eq!(state.screenshot_count(), 0);
        assert!(state.screenshot_paths().is_empty());
    }

    #[test]
    fn test_stop_when_not_running() {
        let mut state = ScreenCaptureState::default();
        // Should not panic
        state.stop();
        assert!(!state.is_running());
    }
}
