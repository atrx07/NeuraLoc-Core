use std::sync::Arc;

use rusqlite::{params, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};

use crate::{
    engines::traits::Usage,
    errors::{AppError, AppResult},
    storage::Database,
};

use super::types::{
    BeginTurnInput, ConversationDetail, ConversationMessage, ConversationMessageRole,
    ConversationMessageState, ConversationRecord, ConversationSummary,
};

const CONVERSATION_COLUMNS: &str = "
    c.id, c.title, c.model_id, m.display_name, c.prompt_version_id,
    p.stable_name, pv.version, c.generation_settings_json, c.context_strategy,
    c.pinned, c.created_at, c.updated_at
";

const MESSAGE_COLUMNS: &str = "
    id, conversation_id, parent_id, role, content_json, token_count, state,
    job_id, usage_json, terminal_reason, COALESCE(position, rowid), created_at,
    COALESCE(updated_at, created_at)
";

pub(crate) struct ConversationRepository {
    database: Arc<Database>,
}

pub(crate) struct FinalizeAssistantInput<'a> {
    pub message_id: &'a str,
    pub job_id: &'a str,
    pub content: &'a str,
    pub state: ConversationMessageState,
    pub usage: Option<&'a Usage>,
    pub terminal_reason: Option<&'a str>,
    pub now: &'a str,
}

impl ConversationRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn list(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<ConversationSummary>> {
        let pattern = format!("%{query}%");
        let connection = self.database.connection();
        let mut statement = connection.prepare(&format!(
            "SELECT {CONVERSATION_COLUMNS},
               (SELECT COUNT(*) FROM messages count_messages WHERE count_messages.conversation_id = c.id)
             FROM conversations c
             JOIN models m ON m.id = c.model_id
             LEFT JOIN prompt_versions pv ON pv.id = c.prompt_version_id
             LEFT JOIN prompt_profiles p ON p.id = pv.profile_id
             WHERE (?1 = '' OR c.title LIKE ?2 COLLATE NOCASE
                    OR m.display_name LIKE ?2 COLLATE NOCASE
                    OR COALESCE(p.stable_name, '') LIKE ?2 COLLATE NOCASE)
             ORDER BY c.pinned DESC, c.updated_at DESC, c.id DESC
             LIMIT ?3 OFFSET ?4"
        ))?;
        let stored = statement
            .query_map(
                params![query, pattern, i64::from(limit), i64::from(offset)],
                |row| Ok((StoredConversation::from_row(row)?, row.get::<_, i64>(12)?)),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        stored
            .into_iter()
            .map(|(conversation, count)| conversation.into_summary(count))
            .collect()
    }

    pub fn get(&self, conversation_id: &str) -> AppResult<Option<ConversationDetail>> {
        let connection = self.database.connection();
        let stored = connection
            .query_row(
                &format!(
                    "SELECT {CONVERSATION_COLUMNS}
                     FROM conversations c
                     JOIN models m ON m.id = c.model_id
                     LEFT JOIN prompt_versions pv ON pv.id = c.prompt_version_id
                     LEFT JOIN prompt_profiles p ON p.id = pv.profile_id
                     WHERE c.id = ?1"
                ),
                [conversation_id],
                StoredConversation::from_row,
            )
            .optional()?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let conversation = stored.try_into()?;
        let mut statement = connection.prepare(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM messages
             WHERE conversation_id = ?1
             ORDER BY COALESCE(position, rowid), created_at, id"
        ))?;
        let stored_messages = statement
            .query_map([conversation_id], StoredMessage::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let messages = stored_messages
            .into_iter()
            .map(TryInto::try_into)
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Some(ConversationDetail {
            conversation,
            messages,
        }))
    }

    pub fn begin_turn(&self, input: BeginTurnInput<'_>, now: &str) -> AppResult<()> {
        let settings_json = serde_json::json!({
            "maxOutputTokens": input.max_output_tokens,
        })
        .to_string();
        let title = title_from_message(input.user_content);
        let user_content = serialize_content(input.user_content)?;
        let assistant_content = serialize_content("")?;
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        let existing = transaction
            .query_row(
                "SELECT model_id, prompt_version_id FROM conversations WHERE id = ?1",
                [input.conversation_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        match existing {
            Some((model_id, prompt_version_id)) => {
                if model_id != input.model_id
                    || prompt_version_id.as_deref() != input.prompt_version_id
                {
                    return Err(AppError::Conflict(
                        "the conversation model or prompt binding does not match".into(),
                    ));
                }
            }
            None => {
                transaction.execute(
                    "INSERT INTO conversations(
                       id, title, model_id, prompt_version_id, generation_settings_json,
                       context_strategy, pinned, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'full_history', 0, ?6, ?6)",
                    params![
                        input.conversation_id,
                        title,
                        input.model_id,
                        input.prompt_version_id,
                        settings_json,
                        now,
                    ],
                )?;
            }
        }

        let previous_message_id = transaction
            .query_row(
                "SELECT id FROM messages WHERE conversation_id = ?1
                 ORDER BY COALESCE(position, rowid) DESC, created_at DESC, id DESC LIMIT 1",
                [input.conversation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let next_position: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM messages WHERE conversation_id = ?1",
            [input.conversation_id],
            |row| row.get(0),
        )?;
        insert_message(
            &transaction,
            InsertMessageInput {
                id: input.user_message_id,
                conversation_id: input.conversation_id,
                parent_id: previous_message_id.as_deref(),
                role: ConversationMessageRole::User,
                content_json: &user_content,
                state: ConversationMessageState::Complete,
                job_id: None,
                position: next_position,
                now,
            },
        )?;
        insert_message(
            &transaction,
            InsertMessageInput {
                id: input.assistant_message_id,
                conversation_id: input.conversation_id,
                parent_id: Some(input.user_message_id),
                role: ConversationMessageRole::Assistant,
                content_json: &assistant_content,
                state: ConversationMessageState::Draft,
                job_id: Some(input.job_id),
                position: next_position + 1,
                now,
            },
        )?;
        transaction.execute(
            "UPDATE conversations
             SET generation_settings_json = ?2, updated_at = ?3
             WHERE id = ?1",
            params![input.conversation_id, settings_json, now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn finalize_assistant(
        &self,
        input: FinalizeAssistantInput<'_>,
    ) -> AppResult<ConversationMessage> {
        let content_json = serialize_content(input.content)?;
        let usage_json = input
            .usage
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| {
                AppError::Operation(format!("chat usage could not be serialized: {error}"))
            })?;
        let token_count = input
            .usage
            .map(|value| i64::try_from(value.output_tokens))
            .transpose()
            .map_err(|_| AppError::Operation("chat token usage is too large to store".into()))?;
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        let conversation_id = transaction
            .query_row(
                "SELECT conversation_id FROM messages
                 WHERE id = ?1 AND job_id = ?2 AND role = 'assistant' AND state = 'draft'",
                params![input.message_id, input.job_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| {
                AppError::Conflict("the assistant draft is missing or already finalized".into())
            })?;
        transaction.execute(
            "UPDATE messages
             SET content_json = ?3, token_count = ?4, state = ?5, usage_json = ?6,
                 terminal_reason = ?7, updated_at = ?8
             WHERE id = ?1 AND job_id = ?2",
            params![
                input.message_id,
                input.job_id,
                content_json,
                token_count,
                input.state.as_str(),
                usage_json,
                input.terminal_reason,
                input.now,
            ],
        )?;
        transaction.execute(
            "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
            params![conversation_id, input.now],
        )?;
        transaction.commit()?;
        drop(connection);
        self.message(input.message_id)?.ok_or_else(|| {
            AppError::Operation("the finalized assistant message disappeared".into())
        })
    }

    pub fn recover_interrupted(&self, now: &str) -> AppResult<usize> {
        let connection = self.database.connection();
        let changed = connection.execute(
            "UPDATE messages
             SET state = 'interrupted', terminal_reason = 'application_restarted', updated_at = ?1
             WHERE state = 'draft'",
            [now],
        )?;
        if changed > 0 {
            connection.execute(
                "UPDATE conversations SET updated_at = ?1
                 WHERE id IN (SELECT conversation_id FROM messages WHERE state = 'interrupted' AND updated_at = ?1)",
                [now],
            )?;
        }
        Ok(changed)
    }

    pub fn rename(&self, conversation_id: &str, title: &str, now: &str) -> AppResult<()> {
        let connection = self.database.connection();
        let changed = connection.execute(
            "UPDATE conversations SET title = ?2, updated_at = ?3 WHERE id = ?1",
            params![conversation_id, title, now],
        )?;
        ensure_changed(changed, conversation_id)
    }

    pub fn set_pinned(&self, conversation_id: &str, pinned: bool, now: &str) -> AppResult<()> {
        let connection = self.database.connection();
        let changed = connection.execute(
            "UPDATE conversations SET pinned = ?2, updated_at = ?3 WHERE id = ?1",
            params![conversation_id, i64::from(pinned), now],
        )?;
        ensure_changed(changed, conversation_id)
    }

    pub fn delete(&self, conversation_id: &str) -> AppResult<()> {
        let connection = self.database.connection();
        let changed =
            connection.execute("DELETE FROM conversations WHERE id = ?1", [conversation_id])?;
        ensure_changed(changed, conversation_id)
    }

    fn message(&self, message_id: &str) -> AppResult<Option<ConversationMessage>> {
        let connection = self.database.connection();
        let stored = connection
            .query_row(
                &format!("SELECT {MESSAGE_COLUMNS} FROM messages WHERE id = ?1"),
                [message_id],
                StoredMessage::from_row,
            )
            .optional()?;
        stored.map(TryInto::try_into).transpose()
    }
}

struct InsertMessageInput<'a> {
    id: &'a str,
    conversation_id: &'a str,
    parent_id: Option<&'a str>,
    role: ConversationMessageRole,
    content_json: &'a str,
    state: ConversationMessageState,
    job_id: Option<&'a str>,
    position: i64,
    now: &'a str,
}

fn insert_message(transaction: &Transaction<'_>, input: InsertMessageInput<'_>) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO messages(
           id, conversation_id, parent_id, role, content_json, token_count, pinned,
           created_at, state, job_id, usage_json, terminal_reason, position, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, ?9, ?6, ?7, NULL, NULL, ?8, ?9)",
        params![
            input.id,
            input.conversation_id,
            input.parent_id,
            input.role.as_str(),
            input.content_json,
            input.state.as_str(),
            input.job_id,
            input.position,
            input.now,
        ],
    )?;
    Ok(())
}

fn ensure_changed(changed: usize, conversation_id: &str) -> AppResult<()> {
    if changed == 0 {
        Err(AppError::ConversationNotFound(conversation_id.into()))
    } else {
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TextContent {
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

fn serialize_content(content: &str) -> AppResult<String> {
    serde_json::to_string(&TextContent {
        kind: "text".into(),
        text: content.into(),
    })
    .map_err(|error| {
        AppError::Operation(format!("message content could not be serialized: {error}"))
    })
}

fn title_from_message(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = collapsed.chars().take(80).collect();
    if collapsed.chars().count() > 80 {
        title.push_str("...");
    }
    if title.is_empty() {
        "New conversation".into()
    } else {
        title
    }
}

struct StoredConversation {
    id: String,
    title: String,
    model_id: String,
    model_name: String,
    prompt_version_id: Option<String>,
    prompt_name: Option<String>,
    prompt_version: Option<i64>,
    generation_settings_json: String,
    context_strategy: String,
    pinned: i64,
    created_at: String,
    updated_at: String,
}

impl StoredConversation {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            title: row.get(1)?,
            model_id: row.get(2)?,
            model_name: row.get(3)?,
            prompt_version_id: row.get(4)?,
            prompt_name: row.get(5)?,
            prompt_version: row.get(6)?,
            generation_settings_json: row.get(7)?,
            context_strategy: row.get(8)?,
            pinned: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }

    fn into_summary(self, message_count: i64) -> AppResult<ConversationSummary> {
        Ok(ConversationSummary {
            id: self.id,
            title: self.title,
            model_id: self.model_id,
            model_name: self.model_name,
            prompt_version_id: self.prompt_version_id,
            prompt_name: self.prompt_name,
            prompt_version: optional_u32(self.prompt_version, "prompt version")?,
            context_strategy: self.context_strategy,
            pinned: self.pinned != 0,
            message_count: u32::try_from(message_count)
                .map_err(|_| AppError::Operation("conversation message count is invalid".into()))?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

impl TryFrom<StoredConversation> for ConversationRecord {
    type Error = AppError;

    fn try_from(value: StoredConversation) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            title: value.title,
            model_id: value.model_id,
            model_name: value.model_name,
            prompt_version_id: value.prompt_version_id,
            prompt_name: value.prompt_name,
            prompt_version: optional_u32(value.prompt_version, "prompt version")?,
            generation_settings: serde_json::from_str(&value.generation_settings_json).map_err(
                |error| AppError::Operation(format!("conversation settings are corrupt: {error}")),
            )?,
            context_strategy: value.context_strategy,
            pinned: value.pinned != 0,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

struct StoredMessage {
    id: String,
    conversation_id: String,
    parent_id: Option<String>,
    role: String,
    content_json: String,
    token_count: Option<i64>,
    state: String,
    job_id: Option<String>,
    usage_json: Option<String>,
    terminal_reason: Option<String>,
    position: i64,
    created_at: String,
    updated_at: String,
}

impl StoredMessage {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            parent_id: row.get(2)?,
            role: row.get(3)?,
            content_json: row.get(4)?,
            token_count: row.get(5)?,
            state: row.get(6)?,
            job_id: row.get(7)?,
            usage_json: row.get(8)?,
            terminal_reason: row.get(9)?,
            position: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }
}

impl TryFrom<StoredMessage> for ConversationMessage {
    type Error = AppError;

    fn try_from(value: StoredMessage) -> Result<Self, Self::Error> {
        let content: TextContent = serde_json::from_str(&value.content_json).map_err(|error| {
            AppError::Operation(format!("message {} has corrupt content: {error}", value.id))
        })?;
        if content.kind != "text" {
            return Err(AppError::Operation(format!(
                "message {} has an unsupported content type",
                value.id
            )));
        }
        let usage = value
            .usage_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|error| {
                AppError::Operation(format!("message {} has corrupt usage: {error}", value.id))
            })?;
        Ok(Self {
            id: value.id.clone(),
            conversation_id: value.conversation_id,
            parent_id: value.parent_id,
            role: ConversationMessageRole::parse(&value.role).ok_or_else(|| {
                AppError::Operation(format!("message {} has an unknown role", value.id))
            })?,
            content: content.text,
            state: ConversationMessageState::parse(&value.state).ok_or_else(|| {
                AppError::Operation(format!("message {} has an unknown state", value.id))
            })?,
            job_id: value.job_id,
            token_count: value
                .token_count
                .map(u64::try_from)
                .transpose()
                .map_err(|_| {
                    AppError::Operation(format!("message {} has invalid tokens", value.id))
                })?,
            usage,
            terminal_reason: value.terminal_reason,
            position: u64::try_from(value.position).map_err(|_| {
                AppError::Operation(format!("message {} has an invalid position", value.id))
            })?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

fn optional_u32(value: Option<i64>, label: &str) -> AppResult<Option<u32>> {
    value
        .map(u32::try_from)
        .transpose()
        .map_err(|_| AppError::Operation(format!("the stored {label} is invalid")))
}
