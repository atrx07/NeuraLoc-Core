use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scheduler::resource_policy::FitLabel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    MetadataPending,
    Ready,
    Invalid,
    Missing,
}

impl VerificationState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MetadataPending => "metadata_pending",
            Self::Ready => "ready",
            Self::Invalid => "invalid",
            Self::Missing => "missing",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "metadata_pending" => Some(Self::MetadataPending),
            "ready" => Some(Self::Ready),
            "invalid" => Some(Self::Invalid),
            "missing" => Some(Self::Missing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GgufMetadata {
    pub version: u32,
    pub tensor_count: u64,
    pub metadata_count: u64,
    pub architecture: Option<String>,
    pub name: Option<String>,
    pub file_type: Option<u32>,
    pub quantization: Option<String>,
    pub parameter_count: Option<u64>,
    pub context_length: Option<u64>,
    pub embedding_length: Option<u64>,
    pub layer_count: Option<u64>,
    pub has_chat_template: bool,
    pub metadata_bytes: u64,
    pub metadata_preview: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRecord {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub family: Option<String>,
    pub format: String,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub verification_state: VerificationState,
    pub verification_error: Option<String>,
    pub gguf_metadata: Option<GgufMetadata>,
    pub modified_at_unix_ms: i64,
    pub imported_at: String,
    pub last_verified_at: Option<String>,
    #[serde(skip_serializing)]
    pub(crate) file_identity: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitConfidence {
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFitEstimate {
    pub model_id: String,
    pub route: String,
    pub fit: FitLabel,
    pub confidence: FitConfidence,
    pub context_size: u32,
    pub estimated_ram_bytes: u64,
    pub available_ram_bytes: u64,
    pub reserved_ram_bytes: u64,
    pub weight_bytes: u64,
    pub kv_cache_bytes: u64,
    pub runtime_overhead_bytes: u64,
    pub headroom_bytes: u64,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportModelRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportModelOutcome {
    pub model: ModelRecord,
    pub already_indexed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelIdRequest {
    pub model_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanModelFolderRequest {
    pub scan_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Discovering,
    Importing,
    Complete,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelScanProgress {
    pub scan_id: String,
    pub phase: ScanPhase,
    pub current_path: Option<String>,
    pub discovered: usize,
    pub processed: usize,
    pub imported: usize,
    pub duplicates: usize,
    pub invalid: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelScanIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelScanSummary {
    pub scan_id: String,
    pub discovered: usize,
    pub processed: usize,
    pub imported: usize,
    pub duplicates: usize,
    pub invalid: usize,
    pub cancelled: bool,
    pub issues: Vec<ModelScanIssue>,
}
