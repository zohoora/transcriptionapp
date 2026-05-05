//! ONNX Runtime health IPC.

use crate::{OrtHealth, ORT_HEALTH};

/// Return startup ONNX Runtime health for windows that mount after the
/// `ort_health` event has already fired.
#[tauri::command]
pub fn get_ort_health() -> OrtHealth {
    ORT_HEALTH.get().cloned().unwrap_or(OrtHealth::Missing)
}
