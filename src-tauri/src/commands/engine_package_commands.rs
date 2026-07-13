use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_fs::FsExt;

use crate::{
    app_state::AppState,
    engine_packages::{
        EnginePackageIdRequest, EnginePackageRecord, EnginePackageStatus,
        ImportEnginePackageRequest,
    },
    errors::{AppError, IpcError},
};

#[tauri::command]
pub fn list_engine_packages(
    state: State<'_, AppState>,
) -> Result<Vec<EnginePackageStatus>, IpcError> {
    state.engine_packages.statuses().map_err(Into::into)
}

#[tauri::command]
pub async fn install_engine_package(
    state: State<'_, AppState>,
    request: EnginePackageIdRequest,
) -> Result<EnginePackageRecord, IpcError> {
    state
        .engine_packages
        .install_download(&request.package_id, state.settings.get().internet_access)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn import_engine_package(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportEnginePackageRequest,
) -> Result<EnginePackageRecord, IpcError> {
    if !app.fs_scope().is_allowed(&request.path) {
        return Err(AppError::InvalidPath(
            "the package path was not granted by the native import dialog".into(),
        )
        .into());
    }
    Arc::clone(&state.engine_packages)
        .install_offline(&request.package_id, &request.path)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn verify_engine_package(
    state: State<'_, AppState>,
    request: EnginePackageIdRequest,
) -> Result<EnginePackageRecord, IpcError> {
    state
        .engine_packages
        .verify(&request.package_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn uninstall_engine_package(
    state: State<'_, AppState>,
    request: EnginePackageIdRequest,
) -> Result<(), IpcError> {
    state
        .engine_packages
        .uninstall(&request.package_id)
        .await
        .map_err(Into::into)
}
