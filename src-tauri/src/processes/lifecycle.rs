use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSummary {
    pub id: String,
    pub label: String,
    pub pid: Option<u32>,
    pub state: EngineLifecycle,
    pub started_at: String,
}
