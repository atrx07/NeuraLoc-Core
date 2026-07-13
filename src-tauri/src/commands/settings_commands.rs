use tauri::State;

use crate::{
    app_state::AppState,
    errors::IpcError,
    settings::{AppSettings, SettingsPatch},
};

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.get()
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<AppSettings, IpcError> {
    state.settings.update(patch).map_err(Into::into)
}
