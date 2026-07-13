use std::{collections::BTreeMap, path::PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::errors::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapabilities {
    pub workloads: Vec<String>,
    pub devices: Vec<String>,
    pub formats: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub executable: PathBuf,
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct EngineStartRequest {
    pub model_path: PathBuf,
    pub device_id: String,
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineHandle {
    pub session_id: String,
    pub process_id: String,
    pub endpoint: Option<String>,
    pub backend_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineHealth {
    pub ready: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub job_id: String,
    pub messages_json: String,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenChunk {
    pub job_id: String,
    pub sequence: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub prompt_tokens: u64,
    pub output_tokens: u64,
    pub tokens_per_second: f32,
}

pub type TokenSink = mpsc::Sender<TokenChunk>;

#[async_trait]
pub trait InferenceEngine: Send + Sync {
    fn engine_id(&self) -> &'static str;
    fn capabilities(&self) -> EngineCapabilities;
    async fn prepare(&self, config: EngineConfig) -> AppResult<()>;
    async fn start(&self, request: EngineStartRequest) -> AppResult<EngineHandle>;
    async fn stop(&self) -> AppResult<()>;
    async fn health(&self) -> AppResult<EngineHealth>;
}

#[async_trait]
pub trait ChatEngine: InferenceEngine {
    async fn generate(&self, request: ChatRequest, sink: TokenSink) -> AppResult<Usage>;
    async fn cancel(&self, job_id: &str) -> AppResult<()>;
}
