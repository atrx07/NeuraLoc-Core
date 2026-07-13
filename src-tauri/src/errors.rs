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
        let code = match value {
            AppError::Database(_) => "database_error",
            AppError::Io(_) => "io_error",
            AppError::InvalidSetting(_) => "invalid_setting",
            AppError::Probe(_) => "probe_error",
            AppError::Process(_) => "process_error",
        };
        Self {
            code,
            message: value.to_string(),
            suggestion: Some("Open Logs for details and retry the operation.".into()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
