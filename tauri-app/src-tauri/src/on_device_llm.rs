//! On-Device LLM (Apple Foundation Models) wrapper
//!
//! macOS-only module that provides on-device text generation using Apple's
//! Foundation Models framework (macOS 26+). Used for shadow SOAP note generation
//! alongside the primary LLM Router for quality comparison.
//!
//! On non-macOS platforms or when Swift compilation fails (`no_swift_bridge`),
//! all functions return errors (graceful degradation).

use tracing::{debug, info};

// ============================================================================
// FFI declarations (macOS only, with Swift bridge)
// ============================================================================

#[cfg(all(target_os = "macos", not(cfg_no_swift_bridge)))]
extern "C" {
    fn on_device_llm_check_availability() -> i32;
    fn on_device_llm_generate(
        prompt: *const std::ffi::c_char,
        result: *mut *mut std::ffi::c_char,
        error: *mut *mut std::ffi::c_char,
    ) -> i32;
    fn on_device_llm_free_string(ptr: *mut std::ffi::c_char);
}

// ============================================================================
// Public API
// ============================================================================

/// Client for on-device LLM text generation via Apple Foundation Models.
///
/// Each method call is self-contained — no persistent state is held.
pub struct OnDeviceLLMClient {
    _private: (),
}

impl OnDeviceLLMClient {
    /// Create a new client. Checks availability and returns error if not available.
    #[cfg(all(target_os = "macos", not(cfg_no_swift_bridge)))]
    pub fn new() -> Result<Self, String> {
        let status = unsafe { on_device_llm_check_availability() };
        match status {
            1 => {
                info!("OnDeviceLLMClient initialized (Apple Foundation Models)");
                Ok(Self { _private: () })
            }
            0 => Err("On-device LLM not available (macOS 26+ required)".to_string()),
            _ => Err("On-device LLM check failed".to_string()),
        }
    }

    #[cfg(any(not(target_os = "macos"), cfg_no_swift_bridge))]
    pub fn new() -> Result<Self, String> {
        Err("On-device LLM is only available on macOS 26+ with Apple Silicon".to_string())
    }

    /// Check if on-device LLM is available without creating a client.
    #[cfg(all(target_os = "macos", not(cfg_no_swift_bridge)))]
    pub fn check_availability() -> bool {
        let status = unsafe { on_device_llm_check_availability() };
        status == 1
    }

    #[cfg(any(not(target_os = "macos"), cfg_no_swift_bridge))]
    pub fn check_availability() -> bool {
        false
    }

    /// Generate a SOAP note from a transcript using the on-device model.
    ///
    /// This is a blocking call — should be called from a spawned `std::thread`.
    /// The on-device model is smaller (~3B params) so we use a simplified
    /// plain-text prompt rather than structured JSON.
    #[cfg(all(target_os = "macos", not(cfg_no_swift_bridge)))]
    pub fn generate_soap(&self, transcript: &str, detail_level: u8, _format: &str) -> Result<String, String> {
        use std::ffi::{CStr, CString};

        let prompt = build_on_device_soap_prompt(transcript, detail_level);
        let c_prompt = CString::new(prompt)
            .map_err(|e| format!("Failed to create C string: {}", e))?;

        let mut result_ptr: *mut std::ffi::c_char = std::ptr::null_mut();
        let mut error_ptr: *mut std::ffi::c_char = std::ptr::null_mut();

        let status = unsafe {
            on_device_llm_generate(
                c_prompt.as_ptr(),
                &mut result_ptr,
                &mut error_ptr,
            )
        };

        match status {
            0 => {
                // Success
                let result = if !result_ptr.is_null() {
                    let text = unsafe { CStr::from_ptr(result_ptr) }
                        .to_string_lossy()
                        .to_string();
                    unsafe { on_device_llm_free_string(result_ptr) };
                    text
                } else {
                    String::new()
                };
                debug!("On-device SOAP generated: {} chars", result.len());
                Ok(result)
            }
            1 => {
                // Error
                let error = if !error_ptr.is_null() {
                    let msg = unsafe { CStr::from_ptr(error_ptr) }
                        .to_string_lossy()
                        .to_string();
                    unsafe { on_device_llm_free_string(error_ptr) };
                    msg
                } else {
                    "Unknown error".to_string()
                };
                Err(error)
            }
            2 => {
                // Timeout
                if !error_ptr.is_null() {
                    unsafe { on_device_llm_free_string(error_ptr) };
                }
                Err("On-device LLM generation timed out".to_string())
            }
            _ => Err(format!("Unexpected status code from on-device LLM: {}", status)),
        }
    }

    #[cfg(any(not(target_os = "macos"), cfg_no_swift_bridge))]
    pub fn generate_soap(&self, _transcript: &str, _detail_level: u8, _format: &str) -> Result<String, String> {
        Err("On-device LLM is only available on macOS 26+".to_string())
    }
}

// ============================================================================
// Prompt Building
// ============================================================================

/// Build a simplified SOAP prompt for the on-device model.
///
/// Unlike `llm_client.rs::build_simple_soap_prompt()` which outputs JSON,
/// this produces plain text S:/O:/A:/P: sections — simpler and more reliable
/// for the smaller (~3B param) on-device model.
fn build_on_device_soap_prompt(transcript: &str, detail_level: u8) -> String {
    let detail_guidance = if detail_level <= 3 {
        "Be very brief — one bullet per section."
    } else if detail_level <= 7 {
        "Be concise but thorough."
    } else {
        "Be thorough and detailed."
    };

    format!(
        r#"You are a medical scribe. Generate a clinical SOAP note from this transcript.

Output format (use exactly these section headers):
S:
• [subjective findings]

O:
• [objective findings]

A:
• [assessment/diagnosis]

P:
• [plan items]

Rules:
- Only include information explicitly stated in the transcript
- Use correct medical terminology
- No patient/provider names
- {detail_guidance}

Transcript:
{transcript}"#,
        detail_guidance = detail_guidance,
        transcript = transcript,
    )
}

/// Count the number of SOAP sections (S:/O:/A:/P:) present in generated text.
pub fn count_soap_sections(text: &str) -> usize {
    let mut count = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "S:" || trimmed == "O:" || trimmed == "A:" || trimmed == "P:" {
            count += 1;
        }
    }
    count
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_on_device_soap_prompt_brief() {
        let prompt = build_on_device_soap_prompt("Patient says hello.", 2);
        assert!(prompt.contains("S:"));
        assert!(prompt.contains("O:"));
        assert!(prompt.contains("A:"));
        assert!(prompt.contains("P:"));
        assert!(prompt.contains("very brief"));
        assert!(prompt.contains("Patient says hello."));
    }

    #[test]
    fn test_build_on_device_soap_prompt_medium() {
        let prompt = build_on_device_soap_prompt("Patient has a cough.", 5);
        assert!(prompt.contains("concise but thorough"));
    }

    #[test]
    fn test_build_on_device_soap_prompt_detailed() {
        let prompt = build_on_device_soap_prompt("Long transcript...", 9);
        assert!(prompt.contains("thorough and detailed"));
    }

    #[test]
    fn test_count_soap_sections() {
        let text = "S:\n• Cough for 3 days\n\nO:\n• Lungs clear\n\nA:\n• URI\n\nP:\n• Rest";
        assert_eq!(count_soap_sections(text), 4);
    }

    #[test]
    fn test_count_soap_sections_partial() {
        let text = "S:\n• Headache\n\nA:\n• Migraine";
        assert_eq!(count_soap_sections(text), 2);
    }

    #[test]
    fn test_count_soap_sections_empty() {
        assert_eq!(count_soap_sections(""), 0);
        assert_eq!(count_soap_sections("No sections here"), 0);
    }

    #[cfg(any(not(target_os = "macos"), cfg_no_swift_bridge))]
    #[test]
    fn test_client_not_available_non_macos() {
        assert!(OnDeviceLLMClient::new().is_err());
        assert!(!OnDeviceLLMClient::check_availability());
    }

    #[cfg(all(target_os = "macos", not(cfg_no_swift_bridge)))]
    #[test]
    fn test_client_creation_does_not_panic() {
        // May fail if macOS < 26, but should not panic
        match OnDeviceLLMClient::new() {
            Ok(_) => println!("OnDeviceLLMClient created successfully"),
            Err(e) => println!("OnDeviceLLMClient not available: {}", e),
        }
    }
}
