use tauri::State;

use crate::{app_state::AppState, errors::IpcError, hardware::HardwareSnapshot};

#[tauri::command]
pub async fn get_hardware_snapshot(
    state: State<'_, AppState>,
) -> Result<HardwareSnapshot, IpcError> {
    state.hardware.get().await.map_err(Into::into)
}

#[tauri::command]
pub async fn refresh_hardware(state: State<'_, AppState>) -> Result<HardwareSnapshot, IpcError> {
    state.hardware.refresh().await.map_err(Into::into)
}
