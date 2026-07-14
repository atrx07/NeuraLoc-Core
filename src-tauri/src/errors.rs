use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("The metadata database could not be opened: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("The application data directory is unavailable: {0}")]
    Io(#[from] std::io::Error),
    #[error("The requested setting is invalid: {0}")]
    InvalidSetting(String),
    #[error("The native probe failed: {0}")]
    Probe(String),
    #[error("The process operation failed: {0}")]
    Process(String),
    #[error("The selected path is not allowed: {0}")]
    InvalidPath(String),
    #[error("The GGUF model is invalid: {0}")]
    InvalidModel(String),
    #[error("The model record was not found: {0}")]
    ModelNotFound(String),
    #[error("The engine package operation failed: {0}")]
    EnginePackage(String),
    #[error("The inference engine operation failed: {0}")]
    Engine(String),
    #[error("The operation was cancelled: {0}")]
    Cancelled(String),
    #[error("The operation could not be completed: {0}")]
    Operation(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: &'static str,
    pub message: String,
    pub suggestion: Option<String>,
}

impl From<AppError> for IpcError {
    fn from(value: AppError) -> Self {
        let (code, suggestion) = match &value {
            AppError::Database(_) => (
                "database_error",
                "Open Logs for details and retry the operation.",
            ),
            AppError::Io(_) => (
                "io_error",
                "Check that the file still exists and is readable.",
            ),
            AppError::InvalidSetting(_) => {
                ("invalid_setting", "Review the setting value and try again.")
            }
            AppError::Probe(_) => (
                "probe_error",
                "Open Logs for details and retry the hardware probe.",
            ),
            AppError::Process(_) => (
                "process_error",
                "Open Logs for details and retry the operation.",
            ),
            AppError::InvalidPath(_) => (
                "invalid_path",
                "Choose a regular local file or folder through the import dialog.",
            ),
            AppError::InvalidModel(_) => (
                "invalid_model",
                "Choose a readable GGUF model file and verify the download completed.",
            ),
            AppError::ModelNotFound(_) => (
                "model_not_found",
                "Refresh the model library and try again.",
            ),
            AppError::EnginePackage(_) => (
                "engine_package_error",
                "Check the package status and Logs, then retry or use a verified offline archive.",
            ),
            AppError::Engine(_) => (
                "engine_error",
                "Check the model, runtime status, and retained engine logs, then retry.",
            ),
            AppError::Cancelled(_) => ("cancelled", "Start another operation when you are ready."),
            AppError::Operation(_) => (
                "operation_error",
                "Retry the operation. Open Logs if the problem continues.",
            ),
        };
        Self {
            code,
            message: value.to_string(),
            suggestion: Some(suggestion.into()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
