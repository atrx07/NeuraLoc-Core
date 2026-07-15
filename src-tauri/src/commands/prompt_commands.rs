use tauri::{AppHandle, State};
use tauri_plugin_fs::FsExt;

use crate::{
    app_state::AppState,
    errors::{AppError, IpcError},
    prompts::{
        CompilePromptRequest, CompiledPrompt, CreatePromptRequest, DuplicatePromptRequest,
        ExportPromptRequest, ImportPromptRequest, ListPromptsRequest, PromptExport,
        PromptMutationOutcome, PromptProfileIdRequest, PromptSummary, PromptVersionIdRequest,
        PromptVersionRecord, SavePromptVersionRequest, SetPromptPinnedRequest,
    },
};

#[tauri::command]
pub fn list_prompts(
    state: State<'_, AppState>,
    request: ListPromptsRequest,
) -> Result<Vec<PromptSummary>, IpcError> {
    state
        .prompts
        .list(request.query.as_deref())
        .map_err(Into::into)
}

#[tauri::command]
pub fn import_prompt(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportPromptRequest,
) -> Result<PromptMutationOutcome, IpcError> {
    ensure_dialog_grant(&app, &request.path)?;
    state.prompts.import(request).map_err(Into::into)
}

#[tauri::command]
pub fn create_prompt(
    state: State<'_, AppState>,
    request: CreatePromptRequest,
) -> Result<PromptMutationOutcome, IpcError> {
    state.prompts.create(request).map_err(Into::into)
}

#[tauri::command]
pub fn save_prompt(
    state: State<'_, AppState>,
    request: SavePromptVersionRequest,
) -> Result<PromptMutationOutcome, IpcError> {
    state.prompts.save_version(request).map_err(Into::into)
}

#[tauri::command]
pub fn get_prompt_version(
    state: State<'_, AppState>,
    request: PromptVersionIdRequest,
) -> Result<PromptVersionRecord, IpcError> {
    state
        .prompts
        .get_version(&request.version_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn duplicate_prompt(
    state: State<'_, AppState>,
    request: DuplicatePromptRequest,
) -> Result<PromptMutationOutcome, IpcError> {
    state.prompts.duplicate(request).map_err(Into::into)
}

#[tauri::command]
pub fn delete_prompt(
    state: State<'_, AppState>,
    request: PromptProfileIdRequest,
) -> Result<(), IpcError> {
    state
        .prompts
        .soft_delete(&request.profile_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn set_prompt_pinned(
    state: State<'_, AppState>,
    request: SetPromptPinnedRequest,
) -> Result<PromptSummary, IpcError> {
    state
        .prompts
        .set_pinned(&request.profile_id, request.pinned)
        .map_err(Into::into)
}

#[tauri::command]
pub fn export_prompt(
    state: State<'_, AppState>,
    request: ExportPromptRequest,
) -> Result<PromptExport, IpcError> {
    state.prompts.export(request).map_err(Into::into)
}

#[tauri::command]
pub fn compile_prompt(
    state: State<'_, AppState>,
    request: CompilePromptRequest,
) -> Result<CompiledPrompt, IpcError> {
    state
        .prompts
        .compile(&request.version_id)
        .map_err(Into::into)
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
