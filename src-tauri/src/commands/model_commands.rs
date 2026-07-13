use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_fs::FsExt;

use crate::{
    app_state::AppState,
    errors::{AppError, IpcError},
    models::{
        ImportModelOutcome, ImportModelRequest, ModelIdRequest, ModelRecord, ModelScanSummary,
        ScanModelFolderRequest,
    },
};

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Result<Vec<ModelRecord>, IpcError> {
    state.models.list().map_err(Into::into)
}

#[tauri::command]
pub async fn import_model(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportModelRequest,
) -> Result<ImportModelOutcome, IpcError> {
    ensure_dialog_grant(&app, &request.path)?;
    let models = Arc::clone(&state.models);
    tauri::async_runtime::spawn_blocking(move || models.import_model(&request.path))
        .await
        .map_err(task_error)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn reverify_model(
    state: State<'_, AppState>,
    request: ModelIdRequest,
) -> Result<ModelRecord, IpcError> {
    let models = Arc::clone(&state.models);
    tauri::async_runtime::spawn_blocking(move || models.reverify(&request.model_id))
        .await
        .map_err(task_error)?
        .map_err(Into::into)
}

#[tauri::command]
pub fn remove_model_record(
    state: State<'_, AppState>,
    request: ModelIdRequest,
) -> Result<(), IpcError> {
    state
        .models
        .remove_record(&request.model_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn scan_model_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ScanModelFolderRequest,
) -> Result<ModelScanSummary, IpcError> {
    ensure_dialog_grant(&app, &request.path)?;
    let models = Arc::clone(&state.models);
    let events = Arc::clone(&state.events);
    let stream_id = request.scan_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = models.scan_folder(request, |progress| {
            if let Err(error) = events.emit(&app, "model://scan-progress", &stream_id, progress) {
                tracing::warn!(%error, "model scan progress event failed");
            }
        });
        events.clear_stream("model://scan-progress", &stream_id);
        result
    })
    .await
    .map_err(task_error)?
    .map_err(Into::into)
}

#[tauri::command]
pub fn cancel_model_scan(state: State<'_, AppState>, scan_id: String) -> bool {
    state.models.cancel_scan(&scan_id)
}

fn task_error(error: impl std::fmt::Display) -> IpcError {
    AppError::Operation(format!(
        "the background model task stopped unexpectedly: {error}"
    ))
    .into()
}

fn ensure_dialog_grant(app: &AppHandle, path: &str) -> Result<(), IpcError> {
    if app.fs_scope().is_allowed(path) {
        Ok(())
    } else {
        Err(
            AppError::InvalidPath("the path was not granted by the native import dialog".into())
                .into(),
        )
    }
}
