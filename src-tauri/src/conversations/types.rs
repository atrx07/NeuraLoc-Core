use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engines::traits::Usage;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub model_id: String,
    pub model_name: String,
    pub prompt_version_id: Option<String>,
    pub prompt_name: Option<String>,
    pub prompt_version: Option<u32>,
    pub context_strategy: String,
    pub pinned: bool,
    pub message_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub id: String,
    pub title: String,
    pub model_id: String,
    pub model_name: String,
    pub prompt_version_id: Option<String>,
    pub prompt_name: Option<String>,
    pub prompt_version: Option<u32>,
    pub generation_settings: Value,
    pub context_strategy: String,
    pub pinned: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationMessageRole {
    User,
    Assistant,
}

impl ConversationMessageRole {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationMessageState {
    Complete,
    Draft,
    Cancelled,
    Failed,
    Interrupted,
}

impl ConversationMessageState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Draft => "draft",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "complete" => Some(Self::Complete),
            "draft" => Some(Self::Draft),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub conversation_id: String,
    pub parent_id: Option<String>,
    pub role: ConversationMessageRole,
    pub content: String,
    pub state: ConversationMessageState,
    pub job_id: Option<String>,
    pub token_count: Option<u64>,
    pub usage: Option<Usage>,
    pub terminal_reason: Option<String>,
    pub position: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetail {
    pub conversation: ConversationRecord,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListConversationsRequest {
    pub query: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationIdRequest {
    pub conversation_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameConversationRequest {
    pub conversation_id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetConversationPinnedRequest {
    pub conversation_id: String,
    pub pinned: bool,
}

#[derive(Debug)]
pub(crate) struct BeginTurnInput<'a> {
    pub conversation_id: &'a str,
    pub user_message_id: &'a str,
    pub assistant_message_id: &'a str,
    pub job_id: &'a str,
    pub model_id: &'a str,
    pub prompt_version_id: Option<&'a str>,
    pub user_content: &'a str,
    pub max_output_tokens: u32,
}
