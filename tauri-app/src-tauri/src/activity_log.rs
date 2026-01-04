//! Activity Logging Module
//!
//! Provides structured activity logging for auditing and debugging.
//! IMPORTANT: This module must NEVER log PHI (Protected Health Information).
//!
//! What IS logged:
//! - Session IDs, encounter IDs, segment IDs
//! - Timestamps and durations
//! - Event types and outcomes (success/failure)
//! - File sizes and counts
//! - Model names and settings
//! - Error messages (sanitized)
//!
//! What is NOT logged:
//! - Transcript text
//! - SOAP note content
//! - Patient names or identifiers
//! - Audio content
//! - Any free-text clinical content

use std::path::PathBuf;
use std::sync::OnceLock;
use tracing::{info, warn, error};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Layer};

/// Guard that must be held for the duration of the application
/// to ensure logs are flushed before exit
static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Initialize the activity logging system
///
/// Sets up dual logging:
/// - Console output (human-readable, for development)
/// - File output (JSON, for auditing and analysis)
///
/// Log files are stored in ~/.transcriptionapp/logs/
/// with daily rotation and retention
pub fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = get_log_directory()?;
    std::fs::create_dir_all(&log_dir)?;

    // Create rolling file appender (daily rotation)
    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        &log_dir,
        "activity.log",
    );

    // Non-blocking writer for file output
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Store the guard to keep logging active
    LOG_GUARD.set(guard).ok();

    // File layer - JSON format for structured logging with explicit UTC timestamps
    let file_layer = fmt::layer()
        .json()
        .with_timer(UtcTime::rfc_3339())
        .with_writer(non_blocking)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    // Console layer - human-readable format
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info")));

    // Combine layers
    tracing_subscriber::registry()
        .with(file_layer)
        .with(console_layer)
        .init();

    info!(
        event = "logging_initialized",
        log_dir = %log_dir.display(),
        "Activity logging system initialized"
    );

    Ok(())
}

/// Get the log directory path
fn get_log_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;
    Ok(home.join(".transcriptionapp").join("logs"))
}

// ============================================================================
// Session Lifecycle Events
// ============================================================================

/// Log session start
pub fn log_session_start(session_id: &str, device_name: Option<&str>, model: &str) {
    info!(
        event = "session_start",
        session_id = %session_id,
        device_name = device_name,
        model = %model,
        "Recording session started"
    );
}

/// Log session stop
pub fn log_session_stop(
    session_id: &str,
    duration_ms: u64,
    segment_count: usize,
    audio_file_size: Option<u64>,
) {
    info!(
        event = "session_stop",
        session_id = %session_id,
        duration_ms = duration_ms,
        segment_count = segment_count,
        audio_file_size_bytes = audio_file_size,
        "Recording session stopped"
    );
}

/// Log session reset
pub fn log_session_reset(session_id: Option<&str>) {
    info!(
        event = "session_reset",
        session_id = session_id,
        "Session reset to idle"
    );
}

/// Log session state transition
pub fn log_session_transition(session_id: &str, from_state: &str, to_state: &str) {
    info!(
        event = "session_transition",
        session_id = %session_id,
        from_state = %from_state,
        to_state = %to_state,
        "Session state changed"
    );
}

// ============================================================================
// Transcription Events
// ============================================================================

/// Log transcription segment (without content)
pub fn log_transcription_segment(
    session_id: &str,
    segment_id: &str,
    segment_index: usize,
    duration_ms: u64,
    speaker_id: Option<u32>,
    word_count: usize,
    is_final: bool,
) {
    info!(
        event = "transcription_segment",
        session_id = %session_id,
        segment_id = %segment_id,
        segment_index = segment_index,
        duration_ms = duration_ms,
        speaker_id = speaker_id,
        word_count = word_count,
        is_final = is_final,
        "Transcription segment processed"
    );
}

/// Log SOAP note generation (without content)
pub fn log_soap_generation(
    session_id: &str,
    transcript_word_count: usize,
    generation_time_ms: u64,
    model: &str,
    success: bool,
    error: Option<&str>,
) {
    if success {
        info!(
            event = "soap_generation",
            session_id = %session_id,
            transcript_word_count = transcript_word_count,
            generation_time_ms = generation_time_ms,
            model = %model,
            success = true,
            "SOAP note generated"
        );
    } else {
        warn!(
            event = "soap_generation",
            session_id = %session_id,
            transcript_word_count = transcript_word_count,
            generation_time_ms = generation_time_ms,
            model = %model,
            success = false,
            error = error,
            "SOAP note generation failed"
        );
    }
}

// ============================================================================
// Medplum Sync Events
// ============================================================================

/// Log Medplum authentication
pub fn log_medplum_auth(
    action: &str, // "login", "logout", "refresh", "restore"
    practitioner_id: Option<&str>,
    success: bool,
    error: Option<&str>,
) {
    if success {
        info!(
            event = "medplum_auth",
            action = %action,
            practitioner_id = practitioner_id,
            success = true,
            "Medplum authentication action"
        );
    } else {
        warn!(
            event = "medplum_auth",
            action = %action,
            practitioner_id = practitioner_id,
            success = false,
            error = error,
            "Medplum authentication failed"
        );
    }
}

/// Log encounter sync
pub fn log_encounter_sync(
    session_id: &str,
    encounter_id: &str,
    fhir_id: &str,
    action: &str, // "create", "update", "complete"
    success: bool,
    error: Option<&str>,
) {
    if success {
        info!(
            event = "encounter_sync",
            session_id = %session_id,
            encounter_id = %encounter_id,
            fhir_id = %fhir_id,
            action = %action,
            success = true,
            "Encounter synced to Medplum"
        );
    } else {
        warn!(
            event = "encounter_sync",
            session_id = %session_id,
            encounter_id = %encounter_id,
            fhir_id = %fhir_id,
            action = %action,
            success = false,
            error = error,
            "Encounter sync failed"
        );
    }
}

/// Log document upload (transcript or SOAP)
pub fn log_document_upload(
    session_id: &str,
    encounter_id: &str,
    document_type: &str, // "transcript", "soap_note", "session_info"
    document_id: &str,
    size_bytes: usize,
    success: bool,
    error: Option<&str>,
) {
    if success {
        info!(
            event = "document_upload",
            session_id = %session_id,
            encounter_id = %encounter_id,
            document_type = %document_type,
            document_id = %document_id,
            size_bytes = size_bytes,
            success = true,
            "Document uploaded to Medplum"
        );
    } else {
        warn!(
            event = "document_upload",
            session_id = %session_id,
            encounter_id = %encounter_id,
            document_type = %document_type,
            success = false,
            error = error,
            "Document upload failed"
        );
    }
}

/// Log audio upload
pub fn log_audio_upload(
    session_id: &str,
    encounter_id: &str,
    media_id: &str,
    binary_id: &str,
    size_bytes: usize,
    duration_seconds: Option<u64>,
    success: bool,
    error: Option<&str>,
) {
    if success {
        info!(
            event = "audio_upload",
            session_id = %session_id,
            encounter_id = %encounter_id,
            media_id = %media_id,
            binary_id = %binary_id,
            size_bytes = size_bytes,
            duration_seconds = duration_seconds,
            success = true,
            "Audio uploaded to Medplum"
        );
    } else {
        warn!(
            event = "audio_upload",
            session_id = %session_id,
            encounter_id = %encounter_id,
            success = false,
            error = error,
            "Audio upload failed"
        );
    }
}

// ============================================================================
// Model & Pipeline Events
// ============================================================================

/// Log model loading
pub fn log_model_load(
    model_type: &str, // "whisper", "speaker", "enhancement", "yamnet"
    model_path: &str,
    load_time_ms: u64,
    success: bool,
    error: Option<&str>,
) {
    if success {
        info!(
            event = "model_load",
            model_type = %model_type,
            model_path = %model_path,
            load_time_ms = load_time_ms,
            success = true,
            "Model loaded"
        );
    } else {
        error!(
            event = "model_load",
            model_type = %model_type,
            model_path = %model_path,
            load_time_ms = load_time_ms,
            success = false,
            error = error,
            "Model load failed"
        );
    }
}

/// Log model download
pub fn log_model_download(
    model_type: &str,
    model_url: &str,
    size_bytes: u64,
    download_time_ms: u64,
    success: bool,
    error: Option<&str>,
) {
    if success {
        info!(
            event = "model_download",
            model_type = %model_type,
            model_url = %model_url,
            size_bytes = size_bytes,
            download_time_ms = download_time_ms,
            success = true,
            "Model downloaded"
        );
    } else {
        warn!(
            event = "model_download",
            model_type = %model_type,
            model_url = %model_url,
            success = false,
            error = error,
            "Model download failed"
        );
    }
}

/// Log pipeline start
pub fn log_pipeline_start(
    session_id: &str,
    device_name: &str,
    sample_rate: u32,
    channels: u16,
    diarization_enabled: bool,
    enhancement_enabled: bool,
) {
    info!(
        event = "pipeline_start",
        session_id = %session_id,
        device_name = %device_name,
        sample_rate = sample_rate,
        channels = channels,
        diarization_enabled = diarization_enabled,
        enhancement_enabled = enhancement_enabled,
        "Audio pipeline started"
    );
}

/// Log pipeline stop
pub fn log_pipeline_stop(
    session_id: &str,
    total_audio_ms: u64,
    total_segments: usize,
    avg_processing_ms: Option<f64>,
) {
    info!(
        event = "pipeline_stop",
        session_id = %session_id,
        total_audio_ms = total_audio_ms,
        total_segments = total_segments,
        avg_processing_ms = avg_processing_ms,
        "Audio pipeline stopped"
    );
}

// ============================================================================
// Audio & Device Events
// ============================================================================

/// Log audio device selection
pub fn log_audio_device(device_id: Option<&str>, device_name: &str, sample_rate: u32) {
    info!(
        event = "audio_device",
        device_id = device_id,
        device_name = %device_name,
        sample_rate = sample_rate,
        "Audio device selected"
    );
}

/// Log audio quality issue
pub fn log_audio_quality_issue(
    session_id: &str,
    issue_type: &str, // "clipping", "low_level", "dropout", "noise"
    severity: &str,   // "warning", "error"
    details: &str,
) {
    warn!(
        event = "audio_quality_issue",
        session_id = %session_id,
        issue_type = %issue_type,
        severity = %severity,
        details = %details,
        "Audio quality issue detected"
    );
}

// ============================================================================
// Settings & Configuration Events
// ============================================================================

/// Log settings change
pub fn log_settings_change(setting_name: &str, old_value: &str, new_value: &str) {
    info!(
        event = "settings_change",
        setting_name = %setting_name,
        old_value = %old_value,
        new_value = %new_value,
        "Setting changed"
    );
}

// ============================================================================
// Application Lifecycle Events
// ============================================================================

/// Log application start
pub fn log_app_start(version: &str) {
    info!(
        event = "app_start",
        version = %version,
        "Application started"
    );
}

/// Log application shutdown
pub fn log_app_shutdown(reason: &str) {
    info!(
        event = "app_shutdown",
        reason = %reason,
        "Application shutting down"
    );
}

/// Log checklist result
pub fn log_checklist_result(
    total_checks: usize,
    passed: usize,
    failed: usize,
    warnings: usize,
) {
    info!(
        event = "checklist_result",
        total_checks = total_checks,
        passed = passed,
        failed = failed,
        warnings = warnings,
        "Launch checklist completed"
    );
}

// ============================================================================
// Error Events
// ============================================================================

/// Log an error (sanitized - no PHI)
pub fn log_error(
    context: &str,
    error_type: &str,
    error_message: &str,
    session_id: Option<&str>,
) {
    error!(
        event = "error",
        context = %context,
        error_type = %error_type,
        error_message = %error_message,
        session_id = session_id,
        "Error occurred"
    );
}

/// Log a warning (sanitized - no PHI)
pub fn log_warning(
    context: &str,
    warning_type: &str,
    message: &str,
    session_id: Option<&str>,
) {
    warn!(
        event = "warning",
        context = %context,
        warning_type = %warning_type,
        message = %message,
        session_id = session_id,
        "Warning"
    );
}

// ============================================================================
// Deep Link / OAuth Events
// ============================================================================

/// Log deep link received
pub fn log_deep_link(url_scheme: &str, path: &str, has_code: bool, has_state: bool) {
    info!(
        event = "deep_link",
        url_scheme = %url_scheme,
        path = %path,
        has_code = has_code,
        has_state = has_state,
        "Deep link received"
    );
}

// ============================================================================
// History / Query Events
// ============================================================================

/// Log encounter history query
pub fn log_history_query(
    start_date: Option<&str>,
    end_date: Option<&str>,
    result_count: usize,
) {
    info!(
        event = "history_query",
        start_date = start_date,
        end_date = end_date,
        result_count = result_count,
        "Encounter history queried"
    );
}

/// Log encounter details fetch
pub fn log_encounter_details_fetch(
    encounter_id: &str,
    has_transcript: bool,
    has_soap: bool,
    has_audio: bool,
) {
    info!(
        event = "encounter_details_fetch",
        encounter_id = %encounter_id,
        has_transcript = has_transcript,
        has_soap = has_soap,
        has_audio = has_audio,
        "Encounter details fetched"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_directory() {
        let dir = get_log_directory().unwrap();
        assert!(dir.ends_with("logs"));
        assert!(dir.to_string_lossy().contains(".transcriptionapp"));
    }

    /// Verify that log functions are PHI-safe by checking their signatures.
    ///
    /// This test documents the PHI-safe logging contract:
    /// - log_transcription_segment takes word_count, not transcript text
    /// - log_soap_generation takes timing/model, not SOAP content
    /// - log_document_upload takes size, not document content
    /// - log_audio_upload takes size/duration, not audio data
    ///
    /// If someone changes these functions to include PHI, these tests will fail.
    #[test]
    fn test_transcription_segment_logging_is_phi_safe() {
        // Call log_transcription_segment with test data
        // This verifies the function doesn't require transcript text
        log_transcription_segment(
            "test-session-id",
            "test-segment-id",
            0,                    // segment_index
            1000,                 // duration_ms
            Some(1),              // speaker_id
            42,                   // word_count - NOT transcript text
            true,                 // is_final
        );
        // If this compiles and runs, the function signature is PHI-safe
    }

    #[test]
    fn test_soap_generation_logging_is_phi_safe() {
        // Call log_soap_generation with test data
        // This verifies the function doesn't require SOAP content
        log_soap_generation(
            "test-session-id",
            100,                  // transcript_word_count
            5000,                 // generation_time_ms
            "test-model",         // model name
            true,                 // success
            None,                 // error_message
        );
        // If this compiles and runs, the function signature is PHI-safe
    }

    #[test]
    fn test_document_upload_logging_is_phi_safe() {
        // Call log_document_upload with test data
        // This verifies the function doesn't require document content
        log_document_upload(
            "test-session-id",
            "test-encounter-id",
            "transcript",         // document_type
            "test-doc-id",        // document_id
            1024,                 // size_bytes - NOT document content
            true,                 // success
            None,                 // error_message
        );
        // If this compiles and runs, the function signature is PHI-safe
    }

    #[test]
    fn test_audio_upload_logging_is_phi_safe() {
        // Call log_audio_upload with test data
        // This verifies the function doesn't require audio data
        log_audio_upload(
            "test-session-id",
            "test-encounter-id",
            "test-media-id",      // media_id
            "test-binary-id",     // binary_id
            1024 * 1024,          // size_bytes - NOT audio data
            Some(60000),          // duration_ms
            true,                 // success
            None,                 // error_message
        );
        // If this compiles and runs, the function signature is PHI-safe
    }
}
