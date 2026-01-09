//! Launch Sequence Checklist
//!
//! This module provides a comprehensive pre-flight check system that verifies
//! all requirements are met before starting a recording session.
//!
//! ## Adding New Checks
//!
//! To add a new check for a future feature:
//!
//! 1. Add a new variant to `CheckCategory` if needed
//! 2. Create a check function that returns `CheckResult`
//! 3. Add the check to `run_all_checks()` or the appropriate category function
//! 4. Update the frontend to display the new check
//!
//! ## Example
//!
//! ```rust,ignore
//! fn check_my_new_feature(config: &Config) -> CheckResult {
//!     CheckResult {
//!         id: "my_feature_model".to_string(),
//!         name: "My Feature Model".to_string(),
//!         category: CheckCategory::Model,
//!         status: CheckStatus::Pass,
//!         message: Some("Model loaded successfully".to_string()),
//!         action: None,
//!     }
//! }
//! ```

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::audio;
use crate::config::Config;
use crate::permissions::{self, MicrophoneAuthStatus};

/// Categories for organizing checks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckCategory {
    /// Audio input/output checks
    Audio,
    /// ML model availability checks
    Model,
    /// System permission checks
    Permission,
    /// Configuration validation checks
    Configuration,
    /// Network/connectivity checks (for model downloads)
    Network,
}

/// Status of an individual check
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// Check passed - requirement is met
    Pass,
    /// Check failed - requirement not met, cannot proceed
    Fail,
    /// Warning - can proceed but with degraded functionality
    Warning,
    /// Check is pending/running
    Pending,
    /// Check was skipped (feature disabled)
    Skipped,
}

/// Suggested action to resolve a failed check
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckAction {
    /// Download a model
    DownloadModel { model_name: String },
    /// Open system settings
    OpenSettings { settings_type: String },
    /// Retry the check
    Retry,
    /// No action available
    None,
}

/// Result of a single check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Unique identifier for this check
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Category for grouping
    pub category: CheckCategory,
    /// Current status
    pub status: CheckStatus,
    /// Optional message with details
    pub message: Option<String>,
    /// Suggested action to fix if failed
    pub action: Option<CheckAction>,
}

/// Overall checklist result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistResult {
    /// All individual check results
    pub checks: Vec<CheckResult>,
    /// Whether all required checks passed
    pub all_passed: bool,
    /// Whether the app can start (passed or only warnings)
    pub can_start: bool,
    /// Summary message
    pub summary: String,
}

impl ChecklistResult {
    /// Create a new checklist result from individual checks
    pub fn from_checks(checks: Vec<CheckResult>) -> Self {
        let failed_count = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
        let warning_count = checks.iter().filter(|c| c.status == CheckStatus::Warning).count();
        let passed_count = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
        let total_required = checks.iter().filter(|c| c.status != CheckStatus::Skipped).count();

        let all_passed = failed_count == 0 && warning_count == 0;
        let can_start = failed_count == 0;

        let summary = if all_passed {
            format!("All {} checks passed", passed_count)
        } else if can_start {
            format!("{} passed, {} warnings", passed_count, warning_count)
        } else {
            format!("{} failed, {} warnings out of {} checks", failed_count, warning_count, total_required)
        };

        Self {
            checks,
            all_passed,
            can_start,
            summary,
        }
    }
}

/// Run all pre-flight checks
pub fn run_all_checks(config: &Config) -> ChecklistResult {
    info!("Running launch sequence checklist...");

    let mut checks = Vec::new();

    // Permission checks (FIRST - most critical for user experience)
    checks.extend(run_permission_checks());

    // Audio checks
    checks.extend(run_audio_checks(config));

    // Model checks
    checks.extend(run_model_checks(config));

    // Configuration checks
    checks.extend(run_config_checks(config));

    let result = ChecklistResult::from_checks(checks);

    if result.can_start {
        info!("Checklist complete: {}", result.summary);
    } else {
        warn!("Checklist failed: {}", result.summary);
    }

    result
}

/// Run permission-related checks
fn run_permission_checks() -> Vec<CheckResult> {
    let mut checks = Vec::new();

    // Microphone permission check
    let mic_status = permissions::check_microphone_permission();
    let mic_check = match mic_status {
        MicrophoneAuthStatus::Authorized => CheckResult {
            id: "microphone_permission".to_string(),
            name: "Microphone Permission".to_string(),
            category: CheckCategory::Permission,
            status: CheckStatus::Pass,
            message: Some("Microphone access granted".to_string()),
            action: None,
        },
        MicrophoneAuthStatus::Denied => CheckResult {
            id: "microphone_permission".to_string(),
            name: "Microphone Permission".to_string(),
            category: CheckCategory::Permission,
            status: CheckStatus::Fail,
            message: Some("Microphone access denied. Please grant permission in System Settings → Privacy & Security → Microphone".to_string()),
            action: Some(CheckAction::OpenSettings {
                settings_type: "privacy_microphone".to_string(),
            }),
        },
        MicrophoneAuthStatus::NotDetermined => {
            // Request permission - macOS will show the system dialog
            permissions::request_microphone_permission();
            CheckResult {
                id: "microphone_permission".to_string(),
                name: "Microphone Permission".to_string(),
                category: CheckCategory::Permission,
                status: CheckStatus::Fail,
                message: Some("Microphone permission required. Please allow access when prompted, then try again".to_string()),
                action: Some(CheckAction::Retry),
            }
        }
        MicrophoneAuthStatus::Restricted => CheckResult {
            id: "microphone_permission".to_string(),
            name: "Microphone Permission".to_string(),
            category: CheckCategory::Permission,
            status: CheckStatus::Fail,
            message: Some("Microphone access is restricted by system policy (e.g., parental controls)".to_string()),
            action: None,
        },
        MicrophoneAuthStatus::Unknown => CheckResult {
            id: "microphone_permission".to_string(),
            name: "Microphone Permission".to_string(),
            category: CheckCategory::Permission,
            status: CheckStatus::Warning,
            message: Some("Could not determine microphone permission status".to_string()),
            action: None,
        },
    };
    checks.push(mic_check);

    checks
}

/// Run audio-related checks
fn run_audio_checks(config: &Config) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    // Check if any audio input devices are available
    let devices_check = match audio::list_input_devices() {
        Ok(devices) => {
            if devices.is_empty() {
                CheckResult {
                    id: "audio_devices".to_string(),
                    name: "Audio Input Devices".to_string(),
                    category: CheckCategory::Audio,
                    status: CheckStatus::Fail,
                    message: Some("No audio input devices found".to_string()),
                    action: Some(CheckAction::OpenSettings {
                        settings_type: "sound".to_string()
                    }),
                }
            } else {
                let _default_count = devices.iter().filter(|d| d.is_default).count();
                CheckResult {
                    id: "audio_devices".to_string(),
                    name: "Audio Input Devices".to_string(),
                    category: CheckCategory::Audio,
                    status: CheckStatus::Pass,
                    message: Some(format!("{} device(s) available", devices.len())),
                    action: None,
                }
            }
        }
        Err(e) => {
            CheckResult {
                id: "audio_devices".to_string(),
                name: "Audio Input Devices".to_string(),
                category: CheckCategory::Audio,
                status: CheckStatus::Fail,
                message: Some(format!("Failed to enumerate devices: {}", e)),
                action: Some(CheckAction::OpenSettings {
                    settings_type: "privacy".to_string()
                }),
            }
        }
    };
    checks.push(devices_check);

    // Check if selected device exists (if one is configured)
    if let Some(ref device_id) = config.input_device_id {
        let selected_check = match audio::list_input_devices() {
            Ok(devices) => {
                let found = devices.iter().any(|d| &d.id == device_id);
                if found {
                    CheckResult {
                        id: "selected_device".to_string(),
                        name: "Selected Input Device".to_string(),
                        category: CheckCategory::Audio,
                        status: CheckStatus::Pass,
                        message: Some(format!("Device '{}' available", device_id)),
                        action: None,
                    }
                } else {
                    CheckResult {
                        id: "selected_device".to_string(),
                        name: "Selected Input Device".to_string(),
                        category: CheckCategory::Audio,
                        status: CheckStatus::Warning,
                        message: Some(format!("Device '{}' not found, will use default", device_id)),
                        action: None,
                    }
                }
            }
            Err(_) => CheckResult {
                id: "selected_device".to_string(),
                name: "Selected Input Device".to_string(),
                category: CheckCategory::Audio,
                status: CheckStatus::Skipped,
                message: Some("Could not verify device".to_string()),
                action: None,
            },
        };
        checks.push(selected_check);
    }

    checks
}

/// Run model-related checks
fn run_model_checks(config: &Config) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    // Whisper model check - always remote server
    let whisper_check = CheckResult {
        id: "whisper_model".to_string(),
        name: format!("Whisper Server ({})", config.whisper_server_model),
        category: CheckCategory::Model,
        status: CheckStatus::Pass,
        message: Some(format!("Remote server at {}", config.whisper_server_url)),
        action: None,
    };
    checks.push(whisper_check);

    // Speaker diarization model check (optional based on config)
    let diarization_check = if config.diarization_enabled {
        match config.get_diarization_model_path() {
            Ok(path) => {
                if path.exists() {
                    CheckResult {
                        id: "speaker_model".to_string(),
                        name: "Speaker Diarization Model".to_string(),
                        category: CheckCategory::Model,
                        status: CheckStatus::Pass,
                        message: Some("Model available".to_string()),
                        action: None,
                    }
                } else {
                    CheckResult {
                        id: "speaker_model".to_string(),
                        name: "Speaker Diarization Model".to_string(),
                        category: CheckCategory::Model,
                        status: CheckStatus::Warning,
                        message: Some("Model not found, diarization will be disabled".to_string()),
                        action: Some(CheckAction::DownloadModel {
                            model_name: "speaker_embedding".to_string()
                        }),
                    }
                }
            }
            Err(e) => CheckResult {
                id: "speaker_model".to_string(),
                name: "Speaker Diarization Model".to_string(),
                category: CheckCategory::Model,
                status: CheckStatus::Warning,
                message: Some(format!("Error: {}", e)),
                action: Some(CheckAction::DownloadModel {
                    model_name: "speaker_embedding".to_string()
                }),
            },
        }
    } else {
        CheckResult {
            id: "speaker_model".to_string(),
            name: "Speaker Diarization Model".to_string(),
            category: CheckCategory::Model,
            status: CheckStatus::Skipped,
            message: Some("Diarization disabled".to_string()),
            action: None,
        }
    };
    checks.push(diarization_check);

    // Enhancement model check (optional based on config)
    let enhancement_check = if config.enhancement_enabled {
        match config.get_enhancement_model_path() {
            Ok(path) => {
                if path.exists() {
                    CheckResult {
                        id: "enhancement_model".to_string(),
                        name: "Speech Enhancement Model (GTCRN)".to_string(),
                        category: CheckCategory::Model,
                        status: CheckStatus::Pass,
                        message: Some("Model available".to_string()),
                        action: None,
                    }
                } else {
                    CheckResult {
                        id: "enhancement_model".to_string(),
                        name: "Speech Enhancement Model (GTCRN)".to_string(),
                        category: CheckCategory::Model,
                        status: CheckStatus::Warning,
                        message: Some("Model not found, enhancement will be disabled".to_string()),
                        action: Some(CheckAction::DownloadModel {
                            model_name: "gtcrn_simple".to_string()
                        }),
                    }
                }
            }
            Err(e) => CheckResult {
                id: "enhancement_model".to_string(),
                name: "Speech Enhancement Model (GTCRN)".to_string(),
                category: CheckCategory::Model,
                status: CheckStatus::Warning,
                message: Some(format!("Error: {}", e)),
                action: Some(CheckAction::DownloadModel {
                    model_name: "gtcrn_simple".to_string()
                }),
            },
        }
    } else {
        CheckResult {
            id: "enhancement_model".to_string(),
            name: "Speech Enhancement Model (GTCRN)".to_string(),
            category: CheckCategory::Model,
            status: CheckStatus::Skipped,
            message: Some("Enhancement disabled".to_string()),
            action: None,
        }
    };
    checks.push(enhancement_check);

    // Biomarker analysis checks (optional based on config)
    let biomarker_check = if config.biomarkers_enabled {
        // Check YAMNet model for cough detection
        match config.get_yamnet_model_path() {
            Ok(path) => {
                if path.exists() {
                    CheckResult {
                        id: "yamnet_model".to_string(),
                        name: "YAMNet Model (Cough Detection)".to_string(),
                        category: CheckCategory::Model,
                        status: CheckStatus::Pass,
                        message: Some("Model available - cough detection enabled".to_string()),
                        action: None,
                    }
                } else {
                    // YAMNet is optional even when biomarkers enabled
                    // Vitality and stability work without it
                    CheckResult {
                        id: "yamnet_model".to_string(),
                        name: "YAMNet Model (Cough Detection)".to_string(),
                        category: CheckCategory::Model,
                        status: CheckStatus::Warning,
                        message: Some("Model not found - cough detection disabled, but vitality/stability metrics available".to_string()),
                        action: Some(CheckAction::DownloadModel {
                            model_name: "yamnet".to_string()
                        }),
                    }
                }
            }
            Err(e) => CheckResult {
                id: "yamnet_model".to_string(),
                name: "YAMNet Model (Cough Detection)".to_string(),
                category: CheckCategory::Model,
                status: CheckStatus::Warning,
                message: Some(format!("Error: {} - vitality/stability metrics still available", e)),
                action: Some(CheckAction::DownloadModel {
                    model_name: "yamnet".to_string()
                }),
            },
        }
    } else {
        CheckResult {
            id: "yamnet_model".to_string(),
            name: "Biomarker Analysis".to_string(),
            category: CheckCategory::Model,
            status: CheckStatus::Skipped,
            message: Some("Biomarker analysis disabled".to_string()),
            action: None,
        }
    };
    checks.push(biomarker_check);

    checks
}

/// Run configuration validation checks
fn run_config_checks(config: &Config) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    // VAD threshold validation
    let vad_check = if config.vad_threshold >= 0.0 && config.vad_threshold <= 1.0 {
        CheckResult {
            id: "vad_threshold".to_string(),
            name: "VAD Threshold".to_string(),
            category: CheckCategory::Configuration,
            status: CheckStatus::Pass,
            message: Some(format!("Set to {:.2}", config.vad_threshold)),
            action: None,
        }
    } else {
        CheckResult {
            id: "vad_threshold".to_string(),
            name: "VAD Threshold".to_string(),
            category: CheckCategory::Configuration,
            status: CheckStatus::Fail,
            message: Some(format!("Invalid value: {} (must be 0.0-1.0)", config.vad_threshold)),
            action: None,
        }
    };
    checks.push(vad_check);

    // Max utterance validation
    let utterance_check = if config.max_utterance_ms <= 29000 && config.max_utterance_ms > config.silence_to_flush_ms {
        CheckResult {
            id: "max_utterance".to_string(),
            name: "Max Utterance Duration".to_string(),
            category: CheckCategory::Configuration,
            status: CheckStatus::Pass,
            message: Some(format!("{}ms (within Whisper limit)", config.max_utterance_ms)),
            action: None,
        }
    } else {
        CheckResult {
            id: "max_utterance".to_string(),
            name: "Max Utterance Duration".to_string(),
            category: CheckCategory::Configuration,
            status: CheckStatus::Warning,
            message: Some(format!("{}ms may cause issues", config.max_utterance_ms)),
            action: None,
        }
    };
    checks.push(utterance_check);

    // Models directory check
    let models_dir_check = match Config::models_dir() {
        Ok(dir) => {
            if dir.exists() {
                CheckResult {
                    id: "models_directory".to_string(),
                    name: "Models Directory".to_string(),
                    category: CheckCategory::Configuration,
                    status: CheckStatus::Pass,
                    message: Some(format!("{:?}", dir)),
                    action: None,
                }
            } else {
                // Try to create it
                match std::fs::create_dir_all(&dir) {
                    Ok(_) => CheckResult {
                        id: "models_directory".to_string(),
                        name: "Models Directory".to_string(),
                        category: CheckCategory::Configuration,
                        status: CheckStatus::Pass,
                        message: Some(format!("Created {:?}", dir)),
                        action: None,
                    },
                    Err(e) => CheckResult {
                        id: "models_directory".to_string(),
                        name: "Models Directory".to_string(),
                        category: CheckCategory::Configuration,
                        status: CheckStatus::Fail,
                        message: Some(format!("Cannot create: {}", e)),
                        action: None,
                    },
                }
            }
        }
        Err(e) => CheckResult {
            id: "models_directory".to_string(),
            name: "Models Directory".to_string(),
            category: CheckCategory::Configuration,
            status: CheckStatus::Fail,
            message: Some(format!("Error: {}", e)),
            action: None,
        },
    };
    checks.push(models_dir_check);

    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checklist_result_all_passed() {
        let checks = vec![
            CheckResult {
                id: "test1".to_string(),
                name: "Test 1".to_string(),
                category: CheckCategory::Audio,
                status: CheckStatus::Pass,
                message: None,
                action: None,
            },
            CheckResult {
                id: "test2".to_string(),
                name: "Test 2".to_string(),
                category: CheckCategory::Model,
                status: CheckStatus::Pass,
                message: None,
                action: None,
            },
        ];

        let result = ChecklistResult::from_checks(checks);
        assert!(result.all_passed);
        assert!(result.can_start);
    }

    #[test]
    fn test_checklist_result_with_warning() {
        let checks = vec![
            CheckResult {
                id: "test1".to_string(),
                name: "Test 1".to_string(),
                category: CheckCategory::Audio,
                status: CheckStatus::Pass,
                message: None,
                action: None,
            },
            CheckResult {
                id: "test2".to_string(),
                name: "Test 2".to_string(),
                category: CheckCategory::Model,
                status: CheckStatus::Warning,
                message: None,
                action: None,
            },
        ];

        let result = ChecklistResult::from_checks(checks);
        assert!(!result.all_passed);
        assert!(result.can_start); // Can still start with warnings
    }

    #[test]
    fn test_checklist_result_with_failure() {
        let checks = vec![
            CheckResult {
                id: "test1".to_string(),
                name: "Test 1".to_string(),
                category: CheckCategory::Audio,
                status: CheckStatus::Pass,
                message: None,
                action: None,
            },
            CheckResult {
                id: "test2".to_string(),
                name: "Test 2".to_string(),
                category: CheckCategory::Model,
                status: CheckStatus::Fail,
                message: None,
                action: None,
            },
        ];

        let result = ChecklistResult::from_checks(checks);
        assert!(!result.all_passed);
        assert!(!result.can_start); // Cannot start with failures
    }

    #[test]
    fn test_skipped_checks_not_counted() {
        let checks = vec![
            CheckResult {
                id: "test1".to_string(),
                name: "Test 1".to_string(),
                category: CheckCategory::Audio,
                status: CheckStatus::Pass,
                message: None,
                action: None,
            },
            CheckResult {
                id: "test2".to_string(),
                name: "Test 2".to_string(),
                category: CheckCategory::Model,
                status: CheckStatus::Skipped,
                message: None,
                action: None,
            },
        ];

        let result = ChecklistResult::from_checks(checks);
        assert!(result.all_passed);
        assert!(result.can_start);
    }
}
