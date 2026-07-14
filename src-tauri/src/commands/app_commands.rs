use serde::Serialize;
use tauri::State;

use crate::{app_state::AppState, errors::IpcError};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    version: &'static str,
    database_ready: bool,
    first_run_complete: bool,
    running_engines: usize,
    active_jobs: usize,
}

#[tauri::command]
pub async fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, IpcError> {
    Ok(AppSnapshot {
        version: env!("CARGO_PKG_VERSION"),
        database_ready: true,
        first_run_complete: false,
        running_engines: state.processes.active_count().await,
        active_jobs: state.engines.active_generation_count().await,
    })
}
