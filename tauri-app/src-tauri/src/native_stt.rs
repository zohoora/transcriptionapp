//! Native STT (Apple SFSpeechRecognizer) wrapper
//!
//! macOS-only module that provides on-device speech recognition using Apple's
//! Speech framework. Used as a shadow transcription alongside the primary STT Router
//! for quality comparison.
//!
//! On non-macOS platforms, all functions return errors (graceful degradation).

use tracing::{debug, info, warn};

/// Ensure the Speech framework is loaded into the process.
///
/// On macOS, ObjC classes are only available after their framework is loaded.
/// The full app links Speech.framework transitively via Tauri/AppKit, but test
/// binaries do not. This function uses `dlopen` to load the framework on demand.
#[cfg(target_os = "macos")]
fn ensure_speech_framework_loaded() {
    use std::sync::Once;
    static LOAD: Once = Once::new();
    LOAD.call_once(|| {
        let path = c"/System/Library/Frameworks/Speech.framework/Speech";
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_GLOBAL) };
        if handle.is_null() {
            warn!("Failed to load Speech.framework via dlopen");
        } else {
            debug!("Speech.framework loaded via dlopen");
        }
    });
}

/// Speech recognition authorization status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechAuthStatus {
    Authorized,
    Denied,
    NotDetermined,
    Restricted,
    Unknown,
}

impl std::fmt::Display for SpeechAuthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpeechAuthStatus::Authorized => write!(f, "authorized"),
            SpeechAuthStatus::Denied => write!(f, "denied"),
            SpeechAuthStatus::NotDetermined => write!(f, "not determined"),
            SpeechAuthStatus::Restricted => write!(f, "restricted"),
            SpeechAuthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Check speech recognition authorization status.
#[cfg(target_os = "macos")]
pub fn check_speech_recognition_permission() -> SpeechAuthStatus {
    use objc2::runtime::AnyClass;

    ensure_speech_framework_loaded();
    debug!("Checking speech recognition permission...");

    let cls = match AnyClass::get(c"SFSpeechRecognizer") {
        Some(c) => c,
        None => {
            warn!("SFSpeechRecognizer class not available");
            return SpeechAuthStatus::Unknown;
        }
    };

    // SFSpeechRecognizerAuthorizationStatus: 0=notDetermined, 1=denied, 2=restricted, 3=authorized
    let status: isize = unsafe { objc2::msg_send![cls, authorizationStatus] };

    let result = match status {
        0 => SpeechAuthStatus::NotDetermined,
        1 => SpeechAuthStatus::Denied,
        2 => SpeechAuthStatus::Restricted,
        3 => SpeechAuthStatus::Authorized,
        _ => SpeechAuthStatus::Unknown,
    };

    info!("Speech recognition permission: {}", result);
    result
}

#[cfg(not(target_os = "macos"))]
pub fn check_speech_recognition_permission() -> SpeechAuthStatus {
    debug!("Non-macOS platform - speech recognition not available");
    SpeechAuthStatus::Unknown
}

/// Request speech recognition permission from the user.
/// Returns true if the request was initiated, false if already determined.
#[cfg(target_os = "macos")]
pub fn request_speech_recognition_permission() -> bool {
    use objc2::runtime::AnyClass;
    use block2::StackBlock;

    let status = check_speech_recognition_permission();
    if status != SpeechAuthStatus::NotDetermined {
        debug!("Speech recognition permission already determined: {}", status);
        return false;
    }

    info!("Requesting speech recognition permission...");

    let cls = match AnyClass::get(c"SFSpeechRecognizer") {
        Some(c) => c,
        None => {
            warn!("SFSpeechRecognizer class not available");
            return false;
        }
    };

    let handler = StackBlock::new(|_status: isize| {
        // Permission result handled on re-check
    });
    let handler = handler.copy();

    unsafe {
        let _: () = objc2::msg_send![cls, requestAuthorization: &*handler];
    }

    true
}

#[cfg(not(target_os = "macos"))]
pub fn request_speech_recognition_permission() -> bool {
    debug!("Non-macOS platform - permission request not needed");
    false
}

/// Native STT client using Apple's SFSpeechRecognizer.
///
/// Each `transcribe_blocking` call creates a fresh recognizer instance.
/// This avoids Send/Sync issues and is fine since utterances are infrequent.
pub struct NativeSttClient {
    _private: (), // Prevent construction outside this module
}

#[cfg(target_os = "macos")]
mod ffi {
    //! Raw FFI helpers for Apple Speech framework.
    //! Uses raw pointers and objc_msgSend to avoid objc2 type system constraints.

    use objc2::ffi::{objc_msgSend, objc_getClass};
    use objc2::runtime::{AnyClass, Sel};
    use std::ffi::c_void;
    use std::ptr;

    /// Get an ObjC class by name, returning null if not found.
    pub fn get_class(name: &std::ffi::CStr) -> *const AnyClass {
        unsafe { objc_getClass(name.as_ptr()).cast() }
    }

    /// Send a message with no arguments, returning a raw object pointer.
    /// Safety: caller must ensure sel is valid for the receiver.
    pub unsafe fn send_msg(receiver: *const c_void, sel: Sel) -> *const c_void {
        let func: unsafe extern "C" fn(*const c_void, Sel) -> *const c_void =
            std::mem::transmute(objc_msgSend as *const c_void);
        func(receiver, sel)
    }

    /// Send a message with no arguments, returning void.
    pub unsafe fn send_msg_void(receiver: *const c_void, sel: Sel) {
        let func: unsafe extern "C" fn(*const c_void, Sel) =
            std::mem::transmute(objc_msgSend as *const c_void);
        func(receiver, sel)
    }

    /// Alloc + init (new) pattern for a class.
    pub unsafe fn alloc_init(cls: *const AnyClass) -> *const c_void {
        let sel_new = Sel::register(c"new");
        send_msg(cls as *const c_void, sel_new)
    }

    /// Get *const c_char from an NSString via UTF8String.
    pub unsafe fn nsstring_to_cstr(nsstring: *const c_void) -> *const std::ffi::c_char {
        let sel = Sel::register(c"UTF8String");
        let func: unsafe extern "C" fn(*const c_void, Sel) -> *const std::ffi::c_char =
            std::mem::transmute(objc_msgSend as *const c_void);
        func(nsstring, sel)
    }

    /// Check isAvailable (returns bool/BOOL).
    pub unsafe fn is_available(obj: *const c_void) -> bool {
        let sel = Sel::register(c"isAvailable");
        let func: unsafe extern "C" fn(*const c_void, Sel) -> bool =
            std::mem::transmute(objc_msgSend as *const c_void);
        func(obj, sel)
    }

    /// Release an object (send -release message).
    pub unsafe fn release(obj: *const c_void) {
        if !obj.is_null() {
            send_msg_void(obj, Sel::register(c"release"));
        }
    }

    /// Retain an object (send -retain message).
    #[allow(dead_code)]
    pub unsafe fn retain(obj: *const c_void) -> *const c_void {
        if obj.is_null() {
            return ptr::null();
        }
        send_msg(obj, Sel::register(c"retain"))
    }
}

impl NativeSttClient {
    /// Create a new NativeSttClient.
    /// Verifies that SFSpeechRecognizer is available on this platform.
    #[cfg(target_os = "macos")]
    pub fn new() -> Result<Self, String> {
        ensure_speech_framework_loaded();

        // Verify the Speech framework classes are available
        let cls = ffi::get_class(c"SFSpeechRecognizer");
        if cls.is_null() {
            return Err("SFSpeechRecognizer class not available".to_string());
        }

        let req_cls = ffi::get_class(c"SFSpeechAudioBufferRecognitionRequest");
        if req_cls.is_null() {
            return Err("SFSpeechAudioBufferRecognitionRequest class not available".to_string());
        }

        // Check that recognizer is available
        unsafe {
            let recognizer = ffi::alloc_init(cls.cast());
            if recognizer.is_null() {
                return Err("Failed to create SFSpeechRecognizer".to_string());
            }
            let available = ffi::is_available(recognizer);
            ffi::release(recognizer);
            if !available {
                return Err("SFSpeechRecognizer is not available (language or network)".to_string());
            }
        }

        info!("NativeSttClient initialized (Apple SFSpeechRecognizer)");
        Ok(Self { _private: () })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn new() -> Result<Self, String> {
        Err("Native STT is only available on macOS".to_string())
    }

    /// Transcribe a single utterance using Apple's on-device speech recognition.
    ///
    /// This is a blocking call that waits up to 30 seconds for results.
    /// The audio should be f32 PCM at the specified sample rate (typically 16kHz).
    #[cfg(target_os = "macos")]
    pub fn transcribe_blocking(&self, audio: &[f32], sample_rate: u32) -> Result<String, String> {
        use objc2::ffi::objc_msgSend;
        use objc2::runtime::Sel;
        use std::ffi::c_void;
        use std::sync::mpsc;
        use std::time::Duration;

        if audio.is_empty() {
            return Ok(String::new());
        }

        let start = std::time::Instant::now();

        unsafe {
            // 1. Create SFSpeechRecognizer
            let recognizer_cls = ffi::get_class(c"SFSpeechRecognizer");
            let recognizer = ffi::alloc_init(recognizer_cls.cast());
            if recognizer.is_null() {
                return Err("Failed to create SFSpeechRecognizer".to_string());
            }

            // 2. Create AVAudioFormat (standard float, mono)
            let format_cls = ffi::get_class(c"AVAudioFormat");
            let format_alloc = ffi::send_msg(format_cls as *const c_void, Sel::register(c"alloc"));
            let sel_init_format = Sel::register(c"initStandardFormatWithSampleRate:channels:");
            let init_fn: unsafe extern "C" fn(*const c_void, Sel, f64, u32) -> *const c_void =
                std::mem::transmute(objc_msgSend as *const c_void);
            let format = init_fn(format_alloc, sel_init_format, sample_rate as f64, 1u32);
            if format.is_null() {
                ffi::release(recognizer);
                return Err("Failed to create AVAudioFormat".to_string());
            }

            // 3. Create AVAudioPCMBuffer
            let buffer_cls = ffi::get_class(c"AVAudioPCMBuffer");
            let buffer_alloc = ffi::send_msg(buffer_cls as *const c_void, Sel::register(c"alloc"));
            let sel_init_buffer = Sel::register(c"initWithPCMFormat:frameCapacity:");
            let init_buf_fn: unsafe extern "C" fn(*const c_void, Sel, *const c_void, u32) -> *const c_void =
                std::mem::transmute(objc_msgSend as *const c_void);
            let frame_count = audio.len() as u32;
            let pcm_buffer = init_buf_fn(buffer_alloc, sel_init_buffer, format, frame_count);
            if pcm_buffer.is_null() {
                ffi::release(format);
                ffi::release(recognizer);
                return Err("Failed to create AVAudioPCMBuffer".to_string());
            }

            // 4. Copy f32 samples into buffer
            let sel_float_data = Sel::register(c"floatChannelData");
            let float_data_fn: unsafe extern "C" fn(*const c_void, Sel) -> *const *mut f32 =
                std::mem::transmute(objc_msgSend as *const c_void);
            let float_channel_data = float_data_fn(pcm_buffer, sel_float_data);
            if float_channel_data.is_null() {
                ffi::release(pcm_buffer);
                ffi::release(format);
                ffi::release(recognizer);
                return Err("floatChannelData is null".to_string());
            }
            let channel_ptr = *float_channel_data;
            if channel_ptr.is_null() {
                ffi::release(pcm_buffer);
                ffi::release(format);
                ffi::release(recognizer);
                return Err("floatChannelData[0] is null".to_string());
            }
            std::ptr::copy_nonoverlapping(audio.as_ptr(), channel_ptr, audio.len());

            // Set frameLength
            let sel_set_frame_length = Sel::register(c"setFrameLength:");
            let set_fl_fn: unsafe extern "C" fn(*const c_void, Sel, u32) =
                std::mem::transmute(objc_msgSend as *const c_void);
            set_fl_fn(pcm_buffer, sel_set_frame_length, frame_count);

            // 5. Create SFSpeechAudioBufferRecognitionRequest
            let request_cls = ffi::get_class(c"SFSpeechAudioBufferRecognitionRequest");
            let request = ffi::alloc_init(request_cls.cast());
            if request.is_null() {
                ffi::release(pcm_buffer);
                ffi::release(format);
                ffi::release(recognizer);
                return Err("Failed to create recognition request".to_string());
            }

            // setShouldReportPartialResults: NO
            let sel_set_partial = Sel::register(c"setShouldReportPartialResults:");
            let set_bool_fn: unsafe extern "C" fn(*const c_void, Sel, bool) =
                std::mem::transmute(objc_msgSend as *const c_void);
            set_bool_fn(request, sel_set_partial, false);

            // setRequiresOnDeviceRecognition: YES
            let sel_set_on_device = Sel::register(c"setRequiresOnDeviceRecognition:");
            set_bool_fn(request, sel_set_on_device, true);

            // 6. Append buffer and end audio
            let sel_append = Sel::register(c"appendAudioPCMBuffer:");
            let append_fn: unsafe extern "C" fn(*const c_void, Sel, *const c_void) =
                std::mem::transmute(objc_msgSend as *const c_void);
            append_fn(request, sel_append, pcm_buffer);

            let sel_end_audio = Sel::register(c"endAudio");
            ffi::send_msg_void(request, sel_end_audio);

            // 7. Create result channel
            let (result_tx, result_rx) = mpsc::channel::<Result<String, String>>();

            // 8. Create completion handler block
            let handler = block2::StackBlock::new(
                move |result_obj: *const c_void, error_obj: *const c_void| {
                    if !error_obj.is_null() {
                        let desc = ffi::send_msg(error_obj, Sel::register(c"localizedDescription"));
                        let c_str = ffi::nsstring_to_cstr(desc);
                        let error_msg = if !c_str.is_null() {
                            std::ffi::CStr::from_ptr(c_str)
                                .to_string_lossy()
                                .to_string()
                        } else {
                            "Unknown speech recognition error".to_string()
                        };
                        let _ = result_tx.send(Err(error_msg));
                        return;
                    }

                    if result_obj.is_null() {
                        let _ = result_tx.send(Ok(String::new()));
                        return;
                    }

                    // Check isFinal
                    let sel_is_final = Sel::register(c"isFinal");
                    let is_final_fn: unsafe extern "C" fn(*const c_void, Sel) -> bool =
                        std::mem::transmute(objc_msgSend as *const c_void);
                    let is_final = is_final_fn(result_obj, sel_is_final);
                    if !is_final {
                        return;
                    }

                    // Get bestTranscription.formattedString
                    let transcription = ffi::send_msg(result_obj, Sel::register(c"bestTranscription"));
                    let formatted_ns = ffi::send_msg(transcription, Sel::register(c"formattedString"));
                    let c_str = ffi::nsstring_to_cstr(formatted_ns);

                    let text = if !c_str.is_null() {
                        std::ffi::CStr::from_ptr(c_str)
                            .to_string_lossy()
                            .to_string()
                    } else {
                        String::new()
                    };

                    let _ = result_tx.send(Ok(text));
                },
            );
            let handler = handler.copy();

            // 9. Start recognition task
            let sel_recognize = Sel::register(c"recognitionTaskWithRequest:resultHandler:");
            let recognize_fn: unsafe extern "C" fn(*const c_void, Sel, *const c_void, *const c_void) -> *const c_void =
                std::mem::transmute(objc_msgSend as *const c_void);
            let _task = recognize_fn(recognizer, sel_recognize, request, (&*handler) as *const _ as *const c_void);

            // 10. Wait for result with 30s timeout
            let result = match result_rx.recv_timeout(Duration::from_secs(30)) {
                Ok(Ok(text)) => {
                    let elapsed = start.elapsed();
                    debug!(
                        "Native STT transcribed {} words in {:.1}s from {:.1}s audio",
                        text.split_whitespace().count(),
                        elapsed.as_secs_f64(),
                        audio.len() as f64 / sample_rate as f64
                    );
                    Ok(text)
                }
                Ok(Err(e)) => {
                    warn!("Native STT error: {}", e);
                    Err(format!("Speech recognition failed: {}", e))
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    warn!("Native STT timed out after 30s");
                    // Cancel the in-progress recognition task to free resources
                    if !_task.is_null() {
                        let sel_cancel = Sel::register(c"cancel");
                        ffi::send_msg_void(_task, sel_cancel);
                    }
                    Err("Speech recognition timed out after 30 seconds".to_string())
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    warn!("Native STT channel disconnected");
                    Err("Speech recognition channel disconnected".to_string())
                }
            };

            // Cleanup
            ffi::release(request);
            ffi::release(pcm_buffer);
            ffi::release(format);
            ffi::release(recognizer);

            result
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn transcribe_blocking(&self, _audio: &[f32], _sample_rate: u32) -> Result<String, String> {
        Err("Native STT is only available on macOS".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speech_auth_status_display() {
        assert_eq!(format!("{}", SpeechAuthStatus::Authorized), "authorized");
        assert_eq!(format!("{}", SpeechAuthStatus::Denied), "denied");
        assert_eq!(format!("{}", SpeechAuthStatus::NotDetermined), "not determined");
        assert_eq!(format!("{}", SpeechAuthStatus::Restricted), "restricted");
        assert_eq!(format!("{}", SpeechAuthStatus::Unknown), "unknown");
    }

    #[test]
    fn test_check_permission_does_not_panic() {
        let status = check_speech_recognition_permission();
        println!("Speech recognition status: {:?}", status);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_native_stt_client_creation() {
        // This may fail if permission is denied, but should not panic
        match NativeSttClient::new() {
            Ok(_) => println!("NativeSttClient created successfully"),
            Err(e) => println!("NativeSttClient creation failed (expected if no permission): {}", e),
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_native_stt_client_not_available() {
        assert!(NativeSttClient::new().is_err());
    }
}
