use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineLifecycle {
    NotInstalled,
    Installed,
    Starting,
    LoadingModel,
    Ready,
    Busy,
    Stopping,
    Stopped,
    Crashed,
    Recovering,
    Error,
}

impl EngineLifecycle {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Stopped | Self::Crashed | Self::Error | Self::NotInstalled
        )
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSummary {
    pub id: String,
    pub label: String,
    pub pid: Option<u32>,
    pub state: EngineLifecycle,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
}
