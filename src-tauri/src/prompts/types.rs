use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMetadata {
    pub name: Option<String>,
    pub declared_version: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub recommended_models: Vec<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub context_reserve: Option<u32>,
    pub collection: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSummary {
    pub profile_id: String,
    pub stable_name: String,
    pub collection: Option<String>,
    pub pinned: bool,
    pub latest_version_id: String,
    pub latest_version: u32,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub source_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptVersionRecord {
    pub id: String,
    pub profile_id: String,
    pub version: u32,
    pub source_path: Option<String>,
    pub source_hash: String,
    pub metadata: PromptMetadata,
    pub content: String,
    pub raw_document: String,
    pub source_profile_id: Option<String>,
    pub source_version_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMutationOutcome {
    pub prompt: PromptSummary,
    pub version: PromptVersionRecord,
    pub already_exists: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListPromptsRequest {
    pub query: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportPromptRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePromptRequest {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePromptVersionRequest {
    pub profile_id: String,
    pub base_version_id: String,
    pub document: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptVersionIdRequest {
    pub version_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicatePromptRequest {
    pub version_id: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptProfileIdRequest {
    pub profile_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetPromptPinnedRequest {
    pub profile_id: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptExportMode {
    Original,
    Normalized,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportPromptRequest {
    pub version_id: String,
    pub mode: PromptExportMode,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptExport {
    pub file_name: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilePromptRequest {
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledPrompt {
    pub version_id: String,
    pub content: String,
    pub estimated_tokens: u32,
    pub approximate: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedPrompt {
    pub metadata: PromptMetadata,
    pub content: String,
    pub raw_document: String,
    pub source_hash: String,
}
