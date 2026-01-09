//! Microphone permission handling for macOS.
//!
//! This module provides cross-platform microphone permission checking,
//! with platform-specific implementations for macOS using AVFoundation.

use tracing::{debug, info, warn};

/// Microphone authorization status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicrophoneAuthStatus {
    /// User has granted microphone access
    Authorized,
    /// User has denied microphone access
    Denied,
    /// User has not yet been asked for permission
    NotDetermined,
    /// Microphone access is restricted (e.g., by parental controls)
    Restricted,
    /// Status unknown or check failed
    Unknown,
}

impl std::fmt::Display for MicrophoneAuthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MicrophoneAuthStatus::Authorized => write!(f, "authorized"),
            MicrophoneAuthStatus::Denied => write!(f, "denied"),
            MicrophoneAuthStatus::NotDetermined => write!(f, "not determined"),
            MicrophoneAuthStatus::Restricted => write!(f, "restricted"),
            MicrophoneAuthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Check microphone authorization status.
///
/// On macOS, this checks AVCaptureDevice authorization status for audio.
/// On other platforms, this returns Authorized (assuming permission is handled at OS level).
#[cfg(target_os = "macos")]
pub fn check_microphone_permission() -> MicrophoneAuthStatus {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio, AVAuthorizationStatus};

    debug!("Checking macOS microphone permission...");

    // AVMediaTypeAudio is Option<&NSString> in newer versions
    // Safety: AVMediaTypeAudio is a static extern, accessing it is safe
    let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
        warn!("AVMediaTypeAudio not available");
        return MicrophoneAuthStatus::Unknown;
    };

    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };

    let result = match status {
        AVAuthorizationStatus::Authorized => MicrophoneAuthStatus::Authorized,
        AVAuthorizationStatus::Denied => MicrophoneAuthStatus::Denied,
        AVAuthorizationStatus::NotDetermined => MicrophoneAuthStatus::NotDetermined,
        AVAuthorizationStatus::Restricted => MicrophoneAuthStatus::Restricted,
        _ => MicrophoneAuthStatus::Unknown,
    };

    info!("Microphone permission status: {}", result);
    result
}

/// Check microphone authorization status (non-macOS fallback).
#[cfg(not(target_os = "macos"))]
pub fn check_microphone_permission() -> MicrophoneAuthStatus {
    debug!("Non-macOS platform - assuming microphone permission granted");
    MicrophoneAuthStatus::Authorized
}

/// Request microphone permission from the user.
///
/// On macOS, this triggers the system permission dialog if status is NotDetermined.
/// Note: Since the permission request is async and handled by macOS, the user
/// should retry after granting permission in the system dialog.
///
/// Returns true if permission was requested, false if already determined.
#[cfg(target_os = "macos")]
pub fn request_microphone_permission() -> bool {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    use objc2::runtime::Bool;
    use block2::ConcreteBlock;

    let status = check_microphone_permission();

    if status == MicrophoneAuthStatus::NotDetermined {
        info!("Requesting microphone permission from user...");

        // Safety: AVMediaTypeAudio is a static extern
        let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
            warn!("AVMediaTypeAudio not available");
            return false;
        };

        // Create a simple completion handler block
        // The block signature expected by AVCaptureDevice is (Bool) -> ()
        let handler = ConcreteBlock::new(|_granted: Bool| {
            // Logging happens on re-check, not here
        });
        let handler = handler.copy();

        unsafe {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(
                media_type,
                &handler,
            );
        }

        true
    } else {
        debug!("Microphone permission already determined: {}", status);
        false
    }
}

/// Request microphone permission (non-macOS fallback).
#[cfg(not(target_os = "macos"))]
pub fn request_microphone_permission() -> bool {
    debug!("Non-macOS platform - permission request not needed");
    false
}

/// Open system settings to the microphone privacy section.
///
/// On macOS, this opens System Settings → Privacy & Security → Microphone.
#[cfg(target_os = "macos")]
pub fn open_microphone_settings() -> anyhow::Result<()> {
    use std::process::Command;

    info!("Opening macOS Privacy & Security settings...");

    // Open System Settings to Privacy & Security > Microphone
    // The URL scheme for this is:
    // x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn()?;

    Ok(())
}

/// Open system settings (non-macOS fallback).
#[cfg(not(target_os = "macos"))]
pub fn open_microphone_settings() -> anyhow::Result<()> {
    warn!("Opening system settings not supported on this platform");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microphone_status_display() {
        assert_eq!(format!("{}", MicrophoneAuthStatus::Authorized), "authorized");
        assert_eq!(format!("{}", MicrophoneAuthStatus::Denied), "denied");
        assert_eq!(format!("{}", MicrophoneAuthStatus::NotDetermined), "not determined");
        assert_eq!(format!("{}", MicrophoneAuthStatus::Restricted), "restricted");
        assert_eq!(format!("{}", MicrophoneAuthStatus::Unknown), "unknown");
    }

    #[test]
    fn test_check_permission_does_not_panic() {
        // Just verify it doesn't panic - actual result depends on system state
        let status = check_microphone_permission();
        println!("Microphone permission status: {:?}", status);
    }
}
