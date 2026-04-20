//! Commands for continuous charting mode (start/stop/status)

use super::CommandError;
use crate::commands::physicians::{
    SharedActivePhysician, SharedProfileClient, SharedRoomConfig, SharedServerConfig,
};
use crate::config::Config;
use crate::continuous_mode::{ContinuousModeHandle, ContinuousModeStats, ContinuousState};
use crate::continuous_mode_events::ContinuousModeEvent;
use crate::server_config_resolve::resolve;
use crate::server_sync::ServerSyncContext;
use chrono::Timelike;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tracing::{info, warn};

/// Resolve the effective (start, end) sleep hours using the precedence rule:
/// compiled default < server < local(only if user-edited).
///
/// Reads the server config lock fresh on every call — the caller is expected
/// to invoke this once per sleep-loop tick so pushes from the profile service
/// take effect without waiting for continuous mode to restart.
async fn resolve_sleep_hours(
    config: &Config,
    server_config: &SharedServerConfig,
) -> (u8, u8) {
    let sc = server_config.read().await;
    let start = resolve(
        Some(&sc.defaults.sleep_start_hour),
        &config.sleep_start_hour,
        "sleep_start_hour",
        &config.user_edited_fields,
    );
    let end = resolve(
        Some(&sc.defaults.sleep_end_hour),
        &config.sleep_end_hour,
        "sleep_end_hour",
        &config.user_edited_fields,
    );
    (start, end)
}

/// Check if the current time falls within the sleep window (EST timezone).
fn is_in_sleep_window(sleep_start: u8, sleep_end: u8) -> bool {
    let hour = chrono::Utc::now()
        .with_timezone(&chrono_tz::America::New_York)
        .hour() as u8;
    is_hour_in_sleep_window(hour, sleep_start, sleep_end)
}

/// Pure function for testability — checks if a given hour falls within the sleep window.
fn is_hour_in_sleep_window(hour: u8, sleep_start: u8, sleep_end: u8) -> bool {
    if sleep_start > sleep_end {
        // Crosses midnight: e.g., 22..6 means hours 22,23,0,1,2,3,4,5
        hour >= sleep_start || hour < sleep_end
    } else {
        hour >= sleep_start && hour < sleep_end
    }
}

/// Compute the next wake-up time as an ISO 8601 string.
fn compute_resume_at(sleep_end: u8) -> String {
    use chrono::TimeZone;
    let now_est = chrono::Utc::now().with_timezone(&chrono_tz::America::New_York);
    let today = now_est.date_naive();
    // If we're past midnight (early morning), resume is today; otherwise tomorrow
    let resume_date = if now_est.hour() < sleep_end as u32 {
        today
    } else {
        today.checked_add_signed(chrono::Duration::days(1)).unwrap_or(today)
    };
    let resume_naive = resume_date
        .and_hms_opt(sleep_end as u32, 0, 0)
        .unwrap();
    let resume_est = match chrono_tz::America::New_York.from_local_datetime(&resume_naive) {
        chrono::LocalResult::Single(dt) => dt,
        chrono::LocalResult::Ambiguous(earliest, _) => earliest,
        chrono::LocalResult::None => {
            // Spring-forward gap: shift forward 1 hour
            let shifted = resume_naive + chrono::Duration::hours(1);
            chrono_tz::America::New_York
                .from_local_datetime(&shifted)
                .earliest()
                .unwrap_or_else(|| shifted.and_utc().with_timezone(&chrono_tz::America::New_York))
        }
    };
    resume_est.with_timezone(&chrono::Utc).to_rfc3339()
}

/// Shared state for the continuous mode handle
pub type SharedContinuousModeState = Arc<Mutex<Option<Arc<ContinuousModeHandle>>>>;

/// Start continuous charting mode
///
/// Starts the audio pipeline and encounter detector loop.
/// Recording runs indefinitely until stop_continuous_mode is called.
/// When sleep mode is enabled, automatically stops at sleep_start_hour (EST)
/// and restarts at sleep_end_hour (EST).
#[tauri::command]
pub async fn start_continuous_mode(
    app: AppHandle,
    continuous_state: State<'_, SharedContinuousModeState>,
    active_physician: State<'_, SharedActivePhysician>,
    room_config_state: State<'_, SharedRoomConfig>,
    profile_client_state: State<'_, SharedProfileClient>,
    server_config: State<'_, SharedServerConfig>,
) -> Result<(), CommandError> {
    info!("Starting continuous charting mode");

    // Check if already running
    {
        let state = continuous_state
            .lock()
            .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
        if state.is_some() {
            return Err(CommandError::AlreadyRunning("continuous mode".into()));
        }
    }

    // Build server sync context from current physician/room state
    let sync_ctx = ServerSyncContext::from_state(
        &active_physician, &room_config_state, &profile_client_state,
    ).await;

    // Create handle — persists across sleep/wake cycles
    let handle = Arc::new(ContinuousModeHandle::new());

    // Store handle in shared state
    {
        let mut state = continuous_state
            .lock()
            .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
        *state = Some(handle.clone());
    }

    // Spawn the continuous mode loop with sleep scheduler
    let handle_for_task = handle.clone();
    let continuous_state_for_cleanup = continuous_state.inner().clone();
    // Clone the Arc<RwLock<ServerConfig>> so every tick can re-read the
    // latest server defaults without snapshotting.
    let server_config_for_task = server_config.inner().clone();

    tokio::spawn(async move {
        // Last-logged sleep hours to avoid spamming the log every tick;
        // we only log when the effective value actually changes.
        let mut last_logged_sleep: Option<(u8, u8)> = None;

        loop {
            // Reload config each cycle (picks up setting changes between sleep/wake)
            let config = Config::load_or_default();

            // Resolve effective sleep hours via precedence rule:
            //   compiled default < server < local(only if user-edited)
            // Each iteration re-reads SharedServerConfig so server pushes take
            // effect within one tick without waiting for continuous-mode restart.
            let (sleep_start, sleep_end) =
                resolve_sleep_hours(&config, &server_config_for_task).await;
            if last_logged_sleep != Some((sleep_start, sleep_end)) {
                info!(
                    "Effective sleep hours: start={} end={} (was={:?})",
                    sleep_start, sleep_end, last_logged_sleep
                );
                last_logged_sleep = Some((sleep_start, sleep_end));
            }

            // Check if we're in the sleep window
            if config.sleep_mode_enabled && is_in_sleep_window(sleep_start, sleep_end) {
                let resume_at = compute_resume_at(sleep_end);
                info!("Entering sleep mode — will resume at {}", resume_at);

                // Update handle state to Sleeping
                if let Ok(mut state) = handle_for_task.state.lock() {
                    *state = ContinuousState::Sleeping;
                }
                if let Ok(mut v) = handle_for_task.sleep_resume_at.lock() {
                    *v = Some(resume_at.clone());
                }

                ContinuousModeEvent::SleepStarted {
                    resume_at: resume_at.clone(),
                }
                .emit(&app);

                // Wait until sleep window ends or user stops
                let mut sleep_poll_count: u32 = 0;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    if handle_for_task.user_stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    sleep_poll_count += 1;
                    // Reload config every 5 minutes to check for setting changes
                    if sleep_poll_count % 10 == 0 {
                        let cfg = Config::load_or_default();
                        let (start, end) =
                            resolve_sleep_hours(&cfg, &server_config_for_task).await;
                        if !cfg.sleep_mode_enabled || !is_in_sleep_window(start, end) {
                            break;
                        }
                    }
                }

                if handle_for_task.user_stop_flag.load(Ordering::Relaxed) {
                    info!("User stopped during sleep mode");
                    break;
                }

                info!("Sleep mode ended — resuming continuous mode");
                ContinuousModeEvent::SleepEnded.emit(&app);
            }

            // Reset handle for a fresh run cycle
            handle_for_task.reset_for_new_run();

            // Spawn a sleep timer that will stop this run at sleep_start_hour.
            // The timer re-resolves each tick so a mid-run server push to the
            // sleep hours propagates within ~60s.
            let sleep_timer_handle = if config.sleep_mode_enabled {
                let stop_flag = handle_for_task.stop_flag.clone();
                let server_cfg = server_config_for_task.clone();
                Some(tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        if stop_flag.load(Ordering::Relaxed) {
                            break; // Already stopped (user or other)
                        }
                        // Re-read each tick — server pushes propagate here too
                        let cfg = Config::load_or_default();
                        let (start, end) = resolve_sleep_hours(&cfg, &server_cfg).await;
                        if is_in_sleep_window(start, end) {
                            info!("Sleep timer: entering sleep window, stopping pipeline");
                            stop_flag.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }))
            } else {
                None
            };

            // Run continuous mode (blocks until stop).
            //
            // TauriRunContext bundles the AppHandle + archive root + LLM
            // backend so the orchestrator reaches all of them through the
            // RunContext trait — same code path production and tests use.
            let archive_root = crate::local_archive::get_archive_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/transcriptionapp-archive"));
            let llm_backend: std::sync::Arc<dyn crate::llm_backend::LlmBackend> =
                match crate::llm_client::LLMClient::new(
                    &config.llm_router_url,
                    &config.llm_api_key,
                    &config.llm_client_id,
                    &config.fast_model,
                ) {
                    Ok(c) => std::sync::Arc::new(c),
                    Err(_) => {
                        // Fallback so TauriRunContext can always be constructed;
                        // orchestrator's internal LLMClient::new calls handle
                        // invalid URLs independently by setting flush_llm_client
                        // to None.
                        std::sync::Arc::new(
                            crate::llm_client::LLMClient::new(
                                "http://localhost:8080", "", "dummy", "dummy",
                            ).expect("localhost URL is valid"),
                        )
                    }
                };
            // Snapshot server config here (in an async context where
            // `.read().await` is safe) rather than inside the orchestrator.
            // Previously TauriRunContext::server_config_snapshot used
            // blocking_read on a tokio::sync::RwLock which deadlocked the
            // detector task — observed in v0.10.39 as "Start Session does
            // nothing + second click says Already running". Fixed in v0.10.40+.
            let server_config_snapshot = std::sync::Arc::new(
                server_config_for_task.read().await.clone(),
            );
            let ctx = crate::run_context::TauriRunContext::new(
                app.clone(),
                archive_root,
                llm_backend,
                server_config_snapshot,
            );
            let result = crate::continuous_mode::run_continuous_mode(
                ctx,
                handle_for_task.clone(),
                config,
                sync_ctx.clone(),
            )
            .await;

            // Cancel sleep timer if still running
            if let Some(h) = sleep_timer_handle {
                h.abort();
            }

            if let Err(e) = &result {
                warn!("Continuous mode exited with error: {}", e);
                break; // Fatal error — exit entirely
            }

            // Check if this was a user-initiated stop
            if handle_for_task.user_stop_flag.load(Ordering::Relaxed) {
                info!("User stopped continuous mode");
                break;
            }

            // Otherwise it was a sleep-triggered stop — loop back
            info!("Sleep-triggered stop — will check sleep window");
        }

        // Clean up shared state when done
        if let Ok(mut state) = continuous_state_for_cleanup.lock() {
            *state = None;
        }
    });

    Ok(())
}

/// Stop continuous charting mode
///
/// Signals the pipeline and encounter detector to stop.
/// Any buffered transcript is flushed as a final encounter check.
#[tauri::command]
pub fn stop_continuous_mode(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<(), CommandError> {
    info!("Stopping continuous charting mode");

    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        // Set user_stop_flag first so the outer sleep loop knows to exit
        handle.user_stop_flag.store(true, Ordering::Relaxed);
        handle.stop();
        Ok(())
    } else {
        Err(CommandError::NotRunning("continuous mode".into()))
    }
}

/// Get the current status of continuous charting mode
#[tauri::command]
pub fn get_continuous_mode_status(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<ContinuousModeStats, CommandError> {
    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        Ok(handle.get_stats())
    } else {
        // Return idle stats when not running
        Ok(ContinuousModeStats {
            state: "idle".to_string(),
            recording_since: String::new(),
            encounters_detected: 0,
            recent_encounters: Vec::new(),
            last_error: None,
            buffer_word_count: 0,
            buffer_started_at: None,
            sensor_connected: None,
            sensor_state: None,
            shadow_mode_active: None,
            shadow_method: None,
            last_shadow_outcome: None,
            is_sleeping: false,
            sleep_resume_at: None,
        })
    }
}

/// Get the full transcript text from the current continuous mode buffer.
/// Used by the patient handout feature which needs the complete transcript,
/// not just the ~500 char preview sent via events.
#[tauri::command]
pub fn get_continuous_transcript(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<String, CommandError> {
    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        let buffer = handle
            .transcript_buffer
            .lock()
            .map_err(|_| CommandError::lock_poisoned("transcript_buffer"))?;
        Ok(buffer.full_text_with_speakers())
    } else {
        Ok(String::new())
    }
}

/// Set per-encounter notes for the current continuous mode encounter
///
/// Notes are passed to SOAP generation and cleared when a new encounter starts.
#[tauri::command]
pub fn set_continuous_encounter_notes(
    notes: String,
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<(), CommandError> {
    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        if let Ok(mut encounter_notes) = handle.encounter_notes.lock() {
            *encounter_notes = notes;
        }
        Ok(())
    } else {
        Err(CommandError::NotRunning("continuous mode".into()))
    }
}

/// Get the current encounter transcript from continuous mode.
///
/// Returns the plain text content of the transcript buffer (text segments
/// joined together without speaker labels or timestamps). Returns an empty
/// string if continuous mode is not active.
#[tauri::command]
pub fn get_current_encounter_transcript(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<String, CommandError> {
    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        let buffer = handle
            .transcript_buffer
            .lock()
            .map_err(|_| CommandError::lock_poisoned("transcript_buffer"))?;
        Ok(buffer.full_text())
    } else {
        Ok(String::new())
    }
}

/// List available serial ports (for presence sensor configuration)
///
/// Returns a list of port names (e.g. `/dev/cu.usbserial-2110`) that can be
/// used with the mmWave presence sensor.
#[tauri::command]
pub fn list_serial_ports() -> Result<Vec<String>, CommandError> {
    let ports = serialport::available_ports()
        .map_err(|e| CommandError::Io(e.to_string()))?;
    Ok(ports
        .into_iter()
        .map(|p| p.port_name)
        .collect())
}

/// Trigger a manual new patient encounter split
///
/// Wakes the encounter detector immediately, bypassing minimum duration and
/// word count guards. If the buffer has any content, it will be archived as
/// an encounter and a new SOAP note generated.
#[tauri::command]
pub fn trigger_new_patient(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<(), CommandError> {
    info!("Manual new patient trigger received");
    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        handle.encounter_manual_trigger.notify_one();
        Ok(())
    } else {
        Err(CommandError::NotRunning("continuous mode".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Sleep window tests ──

    #[test]
    fn test_sleep_window_crosses_midnight() {
        // 22-6 window: hours 22,23,0,1,2,3,4,5 are in window
        assert!(is_hour_in_sleep_window(22, 22, 6));
        assert!(is_hour_in_sleep_window(23, 22, 6));
        assert!(is_hour_in_sleep_window(0, 22, 6));
        assert!(is_hour_in_sleep_window(3, 22, 6));
        assert!(is_hour_in_sleep_window(5, 22, 6));
        // 6 AM is wake time — not in window
        assert!(!is_hour_in_sleep_window(6, 22, 6));
        assert!(!is_hour_in_sleep_window(12, 22, 6));
        assert!(!is_hour_in_sleep_window(21, 22, 6));
    }

    #[test]
    fn test_sleep_window_same_day() {
        // 1-5 window (doesn't cross midnight)
        assert!(is_hour_in_sleep_window(1, 1, 5));
        assert!(is_hour_in_sleep_window(4, 1, 5));
        assert!(!is_hour_in_sleep_window(0, 1, 5));
        assert!(!is_hour_in_sleep_window(5, 1, 5));
        assert!(!is_hour_in_sleep_window(12, 1, 5));
    }

    #[test]
    fn test_sleep_window_boundary_exact_hours() {
        // Start hour is in window, end hour is not
        assert!(is_hour_in_sleep_window(22, 22, 6)); // exactly at start
        assert!(!is_hour_in_sleep_window(6, 22, 6)); // exactly at end (wake time)
    }

    #[test]
    fn test_sleep_window_equal_hours_never_sleeps() {
        // Same start and end = zero-length window
        assert!(!is_hour_in_sleep_window(22, 22, 22));
        assert!(!is_hour_in_sleep_window(0, 0, 0));
        assert!(!is_hour_in_sleep_window(12, 6, 6));
    }

    // ── compute_resume_at + DST tests ──

    #[test]
    fn test_compute_resume_at_returns_valid_rfc3339() {
        let resume = compute_resume_at(6);
        // Should parse as RFC3339
        let parsed = chrono::DateTime::parse_from_rfc3339(&resume);
        assert!(parsed.is_ok(), "compute_resume_at should return valid RFC3339, got: {}", resume);
    }

    #[test]
    fn test_compute_resume_at_is_in_future() {
        let resume_str = compute_resume_at(6);
        let resume = chrono::DateTime::parse_from_rfc3339(&resume_str).unwrap();
        let now = chrono::Utc::now();
        // Resume should always be in the future (next 6 AM EST)
        assert!(
            resume.with_timezone(&chrono::Utc) > now,
            "resume_at {} should be in the future (now: {})",
            resume_str,
            now.to_rfc3339()
        );
    }

    #[test]
    fn test_compute_resume_at_within_24_hours() {
        let resume_str = compute_resume_at(6);
        let resume = chrono::DateTime::parse_from_rfc3339(&resume_str).unwrap();
        let now = chrono::Utc::now();
        let diff = resume.with_timezone(&chrono::Utc) - now;
        // Resume should be within 24 hours (next 6 AM, not next month)
        assert!(
            diff.num_hours() <= 26 && diff.num_hours() >= -2,
            "resume should be within ~24h of now, but is {} hours away",
            diff.num_hours()
        );
    }

    /// Spring-forward DST handling: 2026-03-08 02:00 EST jumps to 03:00 EDT.
    /// If sleep_end_hour is 2 and we computed resume on a date where 2 AM doesn't
    /// exist, the function should not panic and should return a valid time.
    #[test]
    fn test_compute_resume_at_handles_dst_spring_forward_gap() {
        // Verify the LocalResult::None path is handled — pick hour 2 which is the
        // typical DST gap. compute_resume_at internally falls back via shifting +1h.
        // We can't easily inject a fake "now" here, so we just verify the code path
        // doesn't panic across reasonable hours.
        for h in 0..24 {
            let result = compute_resume_at(h);
            assert!(
                chrono::DateTime::parse_from_rfc3339(&result).is_ok(),
                "compute_resume_at({}) returned invalid RFC3339: {}",
                h, result
            );
        }
    }

    #[test]
    fn test_dst_spring_forward_gap_handled_by_chrono_tz() {
        // Direct test: 2026-03-08 02:30 EST is a DST gap (clocks jump 2→3 AM EDT)
        use chrono::TimeZone;
        let gap_naive = chrono::NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()
            .and_hms_opt(2, 30, 0).unwrap();
        let result = chrono_tz::America::New_York.from_local_datetime(&gap_naive);
        // chrono-tz returns LocalResult::None for the DST gap
        assert!(matches!(result, chrono::LocalResult::None),
            "DST gap should produce LocalResult::None");

        // Our compute_resume_at handles this with a +1h shift fallback
        let shifted = gap_naive + chrono::Duration::hours(1);
        let shifted_result = chrono_tz::America::New_York
            .from_local_datetime(&shifted)
            .earliest();
        assert!(shifted_result.is_some(), "shifted time should resolve");
    }

    #[test]
    fn test_dst_fall_back_ambiguous_handled() {
        // 2026-11-01 01:30 EST occurs twice (clocks fall back 2→1 AM EST)
        use chrono::TimeZone;
        let ambig_naive = chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
            .and_hms_opt(1, 30, 0).unwrap();
        let result = chrono_tz::America::New_York.from_local_datetime(&ambig_naive);
        // Verify the LocalResult variant - either Ambiguous (most common) or Single
        // (some chrono-tz versions may resolve differently; both are acceptable)
        assert!(
            matches!(result, chrono::LocalResult::Ambiguous(_, _) | chrono::LocalResult::Single(_)),
            "DST fall-back should produce Ambiguous or Single, got {:?}", result
        );
    }
}
