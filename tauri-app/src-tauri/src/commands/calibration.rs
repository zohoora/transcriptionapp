//! Tauri commands for CO2 sensor calibration.

use super::CommandError;
use crate::co2_calibration::{CalibrationCommand, CalibrationHandle};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tracing::info;

pub type SharedCalibrationState = Arc<Mutex<Option<CalibrationHandle>>>;

#[tauri::command]
pub async fn start_co2_calibration(
    room_id: String,
    sensor_url: String,
    app: AppHandle,
    calibration_state: State<'_, SharedCalibrationState>,
) -> Result<(), CommandError> {
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);

    {
        let mut state = calibration_state
            .lock()
            .map_err(|_| CommandError::lock_poisoned("calibration_state"))?;
        if state.is_some() {
            return Err(CommandError::AlreadyRunning("calibration".into()));
        }
        *state = Some(CalibrationHandle {
            stop_flag: stop_flag.clone(),
            command_tx,
        });
    }

    let calibration_state_for_cleanup = calibration_state.inner().clone();
    tokio::spawn(async move {
        crate::co2_calibration::run_calibration(
            sensor_url, room_id, app, stop_flag, command_rx,
        )
        .await;
        if let Ok(mut state) = calibration_state_for_cleanup.lock() {
            *state = None;
        }
    });

    info!("CO2 calibration started");
    Ok(())
}

#[tauri::command]
pub fn stop_co2_calibration(
    calibration_state: State<'_, SharedCalibrationState>,
) -> Result<(), CommandError> {
    let state = calibration_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("calibration_state"))?;
    if let Some(ref handle) = *state {
        handle.stop();
        Ok(())
    } else {
        Err(CommandError::NotRunning("calibration".into()))
    }
}

#[tauri::command]
pub async fn advance_calibration_phase(
    skip: bool,
    calibration_state: State<'_, SharedCalibrationState>,
) -> Result<(), CommandError> {
    let tx = {
        let state = calibration_state
            .lock()
            .map_err(|_| CommandError::lock_poisoned("calibration_state"))?;
        match *state {
            Some(ref handle) => handle.command_tx.clone(),
            None => return Err(CommandError::NotRunning("calibration".into())),
        }
    };
    tx.send(CalibrationCommand::Advance { skip })
        .await
        .map_err(|_| CommandError::Other("Failed to send command".into()))
}

#[tauri::command]
pub fn get_calibration_status(
    calibration_state: State<'_, SharedCalibrationState>,
) -> Result<Option<bool>, CommandError> {
    let state = calibration_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("calibration_state"))?;
    Ok(Some(state.is_some()))
}
