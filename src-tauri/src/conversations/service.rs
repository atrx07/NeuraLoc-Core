use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;

use crate::{
    engines::traits::Usage,
    errors::{AppError, AppResult},
    storage::Database,
};

use super::{
    repository::{ConversationRepository, FinalizeAssistantInput},
    types::{
        BeginTurnInput, ConversationDetail, ConversationExport, ConversationMessage,
        ConversationMessageRole, ConversationMessageState, ConversationRecord, ConversationSummary,
        ListConversationsRequest, RenameConversationRequest, SetConversationPinnedRequest,
    },
};

const MAX_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_ASSISTANT_BYTES: usize = 1024 * 1024;
const MAX_CONVERSATION_EXPORT_BYTES: usize = 16 * 1024 * 1024;

pub struct ConversationService {
    repository: ConversationRepository,
}

impl ConversationService {
    pub fn new(database: Arc<Database>) -> AppResult<Self> {
        let service = Self {
            repository: ConversationRepository::new(database),
        };
        let recovered = service
            .repository
            .recover_interrupted(&Utc::now().to_rfc3339())?;
        if recovered > 0 {
            tracing::warn!(
                recovered,
                "marked abandoned assistant drafts as interrupted"
            );
        }
        Ok(service)
    }

    pub fn list(&self, request: ListConversationsRequest) -> AppResult<Vec<ConversationSummary>> {
        let query = request.query.unwrap_or_default();
        let query = query.trim();
        if query.chars().count() > 200 {
            return Err(AppError::InvalidConversation(
                "conversation search is limited to 200 characters".into(),
            ));
        }
        let limit = request.limit.unwrap_or(50);
        if !(1..=100).contains(&limit) {
            return Err(AppError::InvalidConversation(
                "conversation pages must contain between 1 and 100 records".into(),
            ));
        }
        let offset = request.offset.unwrap_or(0);
        if offset > 100_000 {
            return Err(AppError::InvalidConversation(
                "conversation pagination offset is too large".into(),
            ));
        }
        self.repository.list(query, limit, offset)
    }

    pub fn get(&self, conversation_id: &str) -> AppResult<ConversationDetail> {
        validate_id(conversation_id, "conversation")?;
        self.repository
            .get(conversation_id)?
            .ok_or_else(|| AppError::ConversationNotFound(conversation_id.into()))
    }

    pub fn rename(&self, request: RenameConversationRequest) -> AppResult<()> {
        validate_id(&request.conversation_id, "conversation")?;
        let title = validate_title(&request.title)?;
        self.repository
            .rename(&request.conversation_id, &title, &Utc::now().to_rfc3339())?;
        Ok(())
    }

    pub fn set_pinned(&self, request: SetConversationPinnedRequest) -> AppResult<()> {
        validate_id(&request.conversation_id, "conversation")?;
        self.repository.set_pinned(
            &request.conversation_id,
            request.pinned,
            &Utc::now().to_rfc3339(),
        )?;
        Ok(())
    }

    pub fn delete(&self, conversation_id: &str) -> AppResult<()> {
        validate_id(conversation_id, "conversation")?;
        self.repository.delete(conversation_id)
    }

    pub fn export(&self, conversation_id: &str) -> AppResult<ConversationExport> {
        let detail = self.get(conversation_id)?;
        Ok(ConversationExport {
            file_name: format!("{}.md", safe_file_stem(&detail.conversation.title)),
            media_type: "text/markdown;charset=utf-8".into(),
            content: conversation_markdown(&detail)?,
        })
    }

    pub(crate) fn begin_turn(&self, input: BeginTurnInput<'_>) -> AppResult<()> {
        for (value, label) in [
            (input.conversation_id, "conversation"),
            (input.user_message_id, "user message"),
            (input.assistant_message_id, "assistant message"),
            (input.job_id, "chat job"),
            (input.model_id, "model"),
        ] {
            validate_id(value, label)?;
        }
        if let Some(prompt_version_id) = input.prompt_version_id {
            validate_id(prompt_version_id, "prompt version")?;
        }
        validate_content(input.user_content, MAX_MESSAGE_BYTES, "user message")?;
        if !(1..=4_096).contains(&input.max_output_tokens) {
            return Err(AppError::InvalidConversation(
                "maximum output tokens must be between 1 and 4096".into(),
            ));
        }
        self.repository.begin_turn(input, &Utc::now().to_rfc3339())
    }

    pub(crate) fn finalize_assistant(
        &self,
        message_id: &str,
        job_id: &str,
        content: &str,
        state: ConversationMessageState,
        usage: Option<&Usage>,
        terminal_reason: Option<&str>,
    ) -> AppResult<ConversationMessage> {
        validate_id(message_id, "assistant message")?;
        validate_id(job_id, "chat job")?;
        if content.len() > MAX_ASSISTANT_BYTES || content.contains('\0') {
            return Err(AppError::InvalidConversation(
                "assistant content exceeds the 1 MiB persistence limit".into(),
            ));
        }
        if matches!(state, ConversationMessageState::Draft) {
            return Err(AppError::InvalidConversation(
                "an assistant draft cannot be finalized as another draft".into(),
            ));
        }
        if terminal_reason.is_some_and(|reason| reason.len() > 4_096 || reason.contains('\0')) {
            return Err(AppError::InvalidConversation(
                "the terminal reason is invalid".into(),
            ));
        }
        self.repository.finalize_assistant(FinalizeAssistantInput {
            message_id,
            job_id,
            content,
            state,
            usage,
            terminal_reason,
            now: &Utc::now().to_rfc3339(),
        })
    }
}

fn validate_id(value: &str, label: &str) -> AppResult<()> {
    if value.trim().is_empty() || value.len() > 128 || value.contains(['\r', '\n', '\0']) {
        return Err(AppError::InvalidConversation(format!(
            "the {label} ID is invalid"
        )));
    }
    Ok(())
}

fn validate_title(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 || value.contains(['\r', '\n', '\0']) {
        return Err(AppError::InvalidConversation(
            "conversation titles must contain 1 to 120 characters".into(),
        ));
    }
    Ok(value.into())
}

fn validate_content(value: &str, max_bytes: usize, label: &str) -> AppResult<()> {
    if value.trim().is_empty() || value.len() > max_bytes || value.contains('\0') {
        return Err(AppError::InvalidConversation(format!(
            "the {label} must be non-empty and no larger than 256 KiB"
        )));
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversationProvenance<'a> {
    schema_version: u32,
    conversation: &'a ConversationRecord,
}

fn conversation_markdown(detail: &ConversationDetail) -> AppResult<String> {
    let provenance = serde_json::to_string_pretty(&ConversationProvenance {
        schema_version: 1,
        conversation: &detail.conversation,
    })
    .map_err(|error| {
        AppError::Operation(format!(
            "conversation provenance could not be exported: {error}"
        ))
    })?;
    let mut output = String::new();
    append_export(&mut output, "# ")?;
    append_export(
        &mut output,
        &detail
            .conversation
            .title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )?;
    append_export(&mut output, "\n\n## Provenance\n\n```json\n")?;
    append_export(&mut output, &provenance)?;
    append_export(&mut output, "\n```\n")?;

    for (index, message) in detail.messages.iter().enumerate() {
        let role = match message.role {
            ConversationMessageRole::User => "User",
            ConversationMessageRole::Assistant => "Assistant",
        };
        append_export(&mut output, &format!("\n## {}. {role}\n\n", index + 1))?;
        if message.content.is_empty() {
            append_export(&mut output, "_No response text was stored._")?;
        } else {
            append_export(&mut output, &message.content)?;
        }
        append_export(
            &mut output,
            &format!(
                "\n\n> State: `{}` | Message: `{}` | Created: `{}`",
                message.state.as_str(),
                message.id,
                message.created_at
            ),
        )?;
        if let Some(parent_id) = &message.parent_id {
            append_export(&mut output, &format!(" | Parent: `{parent_id}`"))?;
        }
        if let Some(usage) = &message.usage {
            append_export(
                &mut output,
                &format!(
                    " | Usage: {} prompt, {} output, {:.1} tok/s",
                    usage.prompt_tokens, usage.output_tokens, usage.tokens_per_second
                ),
            )?;
        }
        if let Some(reason) = &message.terminal_reason {
            let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
            append_export(&mut output, &format!(" | Terminal: `{reason}`"))?;
        }
        append_export(&mut output, "\n")?;
    }
    Ok(output)
}

fn append_export(output: &mut String, value: &str) -> AppResult<()> {
    let size = output.len().checked_add(value.len()).ok_or_else(|| {
        AppError::InvalidConversation("the conversation export is too large".into())
    })?;
    if size > MAX_CONVERSATION_EXPORT_BYTES {
        return Err(AppError::InvalidConversation(
            "conversation exports are limited to 16 MiB".into(),
        ));
    }
    output.push_str(value);
    Ok(())
}

fn safe_file_stem(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let value = value.trim_matches('-');
    if value.is_empty() {
        "conversation".into()
    } else {
        value.chars().take(80).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use uuid::Uuid;

    fn service() -> (ConversationService, Arc<Database>, std::path::PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("neuraloc-conversations-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Arc::new(Database::open(&directory.join("test.db")).unwrap());
        seed_model(&database, "model-1", "Qwen 4B");
        let service = ConversationService::new(Arc::clone(&database)).unwrap();
        (service, database, directory)
    }

    fn seed_model(database: &Database, id: &str, name: &str) {
        database
            .connection()
            .execute(
                "INSERT INTO models(
                   id, kind, display_name, family, format, path, size_bytes,
                   compatibility_json, imported_at, verification_state
                 ) VALUES (?1, 'llm', ?2, 'qwen', 'gguf', ?3, 1024, '{}', ?4, 'ready')",
                params![
                    id,
                    name,
                    format!("C:/models/{id}.gguf"),
                    Utc::now().to_rfc3339()
                ],
            )
            .unwrap();
    }

    fn begin(service: &ConversationService, conversation_id: &str, job_id: &str) {
        service
            .begin_turn(BeginTurnInput {
                conversation_id,
                user_message_id: &format!("{job_id}-user"),
                assistant_message_id: &format!("{job_id}-assistant"),
                job_id,
                model_id: "model-1",
                prompt_version_id: None,
                user_content: "Explain durable local chat.",
                max_output_tokens: 512,
            })
            .unwrap();
    }

    #[test]
    fn persists_and_finalizes_a_turn_with_deterministic_order() {
        let (service, _database, directory) = service();
        begin(&service, "conversation-1", "job-1");
        let draft = service.get("conversation-1").unwrap();
        assert_eq!(draft.messages.len(), 2);
        assert_eq!(draft.messages[0].role, ConversationMessageRole::User);
        assert_eq!(draft.messages[0].position, 1);
        assert_eq!(draft.messages[1].state, ConversationMessageState::Draft);
        assert_eq!(draft.messages[1].parent_id.as_deref(), Some("job-1-user"));

        let usage = Usage {
            prompt_tokens: 21,
            output_tokens: 8,
            tokens_per_second: 14.5,
        };
        service
            .finalize_assistant(
                "job-1-assistant",
                "job-1",
                "SQLite keeps the turn durable.",
                ConversationMessageState::Complete,
                Some(&usage),
                Some("completed"),
            )
            .unwrap();
        let complete = service.get("conversation-1").unwrap();
        assert_eq!(
            complete.messages[1].content,
            "SQLite keeps the turn durable."
        );
        assert_eq!(complete.messages[1].usage.as_ref(), Some(&usage));
        assert_eq!(complete.messages[1].token_count, Some(8));
        drop(service);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn recovers_abandoned_drafts_after_database_reopen() {
        let (service, database, directory) = service();
        begin(&service, "conversation-2", "job-2");
        drop(service);
        drop(database);

        let reopened = Arc::new(Database::open(&directory.join("test.db")).unwrap());
        let recovered = ConversationService::new(reopened).unwrap();
        let detail = recovered.get("conversation-2").unwrap();
        assert_eq!(
            detail.messages[1].state,
            ConversationMessageState::Interrupted
        );
        assert_eq!(
            detail.messages[1].terminal_reason.as_deref(),
            Some("application_restarted")
        );
        drop(recovered);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_binding_changes_without_inserting_partial_turns() {
        let (service, database, directory) = service();
        begin(&service, "conversation-3", "job-3");
        seed_model(&database, "model-2", "Llama 3B");
        let result = service.begin_turn(BeginTurnInput {
            conversation_id: "conversation-3",
            user_message_id: "job-4-user",
            assistant_message_id: "job-4-assistant",
            job_id: "job-4",
            model_id: "model-2",
            prompt_version_id: None,
            user_content: "This must roll back.",
            max_output_tokens: 512,
        });
        assert!(result.is_err());
        assert_eq!(service.get("conversation-3").unwrap().messages.len(), 2);
        drop(service);
        drop(database);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn lists_searches_updates_and_cascade_deletes_conversations() {
        let (service, _database, directory) = service();
        begin(&service, "conversation-4", "job-5");
        service
            .rename(RenameConversationRequest {
                conversation_id: "conversation-4".into(),
                title: "Persistence review".into(),
            })
            .unwrap();
        assert_eq!(
            service.get("conversation-4").unwrap().conversation.title,
            "Persistence review"
        );
        service
            .set_pinned(SetConversationPinnedRequest {
                conversation_id: "conversation-4".into(),
                pinned: true,
            })
            .unwrap();
        let listed = service
            .list(ListConversationsRequest {
                query: Some("Qwen".into()),
                limit: Some(20),
                offset: Some(0),
            })
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].pinned);
        assert_eq!(listed[0].message_count, 2);
        service.delete("conversation-4").unwrap();
        assert!(matches!(
            service.get("conversation-4"),
            Err(AppError::ConversationNotFound(_))
        ));
        drop(service);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn exports_a_bounded_markdown_transcript_with_provenance() {
        let (service, _database, directory) = service();
        begin(&service, "conversation-export", "job-export");
        let usage = Usage {
            prompt_tokens: 21,
            output_tokens: 8,
            tokens_per_second: 14.5,
        };
        service
            .finalize_assistant(
                "job-export-assistant",
                "job-export",
                "SQLite keeps the turn durable.",
                ConversationMessageState::Complete,
                Some(&usage),
                Some("completed"),
            )
            .unwrap();

        let exported = service.export("conversation-export").unwrap();
        assert_eq!(exported.media_type, "text/markdown;charset=utf-8");
        assert_eq!(exported.file_name, "Explain-durable-local-chat.md");
        assert!(exported.content.contains("\"schemaVersion\": 1"));
        assert!(exported.content.contains("\"modelId\": \"model-1\""));
        assert!(exported.content.contains("\"maxOutputTokens\": 512"));
        assert!(exported.content.contains("## 1. User"));
        assert!(exported.content.contains("Explain durable local chat."));
        assert!(exported.content.contains("## 2. Assistant"));
        assert!(exported.content.contains("SQLite keeps the turn durable."));
        assert!(exported.content.contains("21 prompt, 8 output, 14.5 tok/s"));
        drop(service);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_multiline_conversation_titles() {
        assert!(validate_title("first\nsecond").is_err());
        assert_eq!(safe_file_stem("../../"), "conversation");
    }
}
