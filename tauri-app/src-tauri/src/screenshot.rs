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

        // Create temp directory for this session
        let temp_dir = std::env::temp_dir().join(format!(
            "transcriptionapp-screenshots-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        ));
        std::fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

        info!("Starting screen capture to {:?}, interval {}s", temp_dir, interval_secs);

        self.temp_dir = Some(temp_dir.clone());
        self.running.store(true, Ordering::SeqCst);

        // Clear previous screenshots
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

    /// Stop capture and clean up temp files.
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

        // Log where screenshots are stored (not deleting for now)
        if let Some(ref dir) = self.temp_dir {
            let count = self.screenshots.lock().map(|ss| ss.len()).unwrap_or(0);
            info!("Screen capture stopped. {} screenshots saved in {:?}", count, dir);
        }

        if let Ok(mut ss) = self.screenshots.lock() {
            ss.clear();
        }
        self.temp_dir = None;
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
    }
}

/// Capture a screenshot and save it to the temp directory
fn capture_and_save(temp_dir: &Path, screenshots: &Arc<Mutex<Vec<PathBuf>>>) {
    debug!("Attempting screen capture...");
    match capture_screen() {
        Ok(image_data) => {
            let filename = format!(
                "capture-{}.jpg",
                chrono::Utc::now().format("%H%M%S-%3f")
            );
            let path = temp_dir.join(&filename);

            match save_jpeg(&image_data, &path) {
                Ok(()) => {
                    debug!("Screenshot saved: {:?} ({}x{}, {} bytes)",
                           path, image_data.width, image_data.height,
                           std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));
                    if let Ok(mut ss) = screenshots.lock() {
                        ss.push(path);
                    }
                }
                Err(e) => warn!("Failed to save screenshot: {}", e),
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
