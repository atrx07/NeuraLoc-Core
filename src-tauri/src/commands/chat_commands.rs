use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tokio::sync::mpsc;

use crate::{
    app_state::AppState,
    engines::traits::{ChatRequest, TokenChunk, Usage},
    errors::{AppError, AppResult, IpcError},
    events::EventEmitter,
    processes::EngineLifecycle,
};

const MAX_MESSAGES: usize = 128;
const MAX_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_TOTAL_MESSAGE_BYTES: usize = 768 * 1024;
const TOKEN_BATCH_INTERVAL: Duration = Duration::from_millis(16);
const TOKEN_BATCH_BYTES: usize = 256;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChatMessage {
    role: ChatRole,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartChatGenerationRequest {
    job_id: String,
    conversation_id: String,
    message_id: String,
    session_id: String,
    messages: Vec<ChatMessage>,
    max_output_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelChatGenerationRequest {
    job_id: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatGenerationState {
    Started,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStateEvent {
    job_id: String,
    conversation_id: String,
    message_id: String,
    state: ChatGenerationState,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTokenBatch {
    job_id: String,
    conversation_id: String,
    message_id: String,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatUsageEvent {
    job_id: String,
    conversation_id: String,
    message_id: String,
    usage: Usage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGenerationResult {
    state: ChatGenerationState,
    usage: Option<Usage>,
}

#[tauri::command]
pub async fn start_chat_generation(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartChatGenerationRequest,
) -> Result<ChatGenerationResult, IpcError> {
    validate_request(&request)?;
    let runtime = state.engines.status().await?;
    if runtime.lifecycle != EngineLifecycle::Ready
        || runtime.session_id.as_deref() != Some(&request.session_id)
    {
        return Err(
            AppError::Engine("the selected model session is no longer ready".into()).into(),
        );
    }

    emit_state(
        &app,
        &state.events,
        &request,
        ChatGenerationState::Started,
        None,
    )?;
    let messages_json = serde_json::to_string(&request.messages).map_err(|error| {
        AppError::Operation(format!("chat messages could not be encoded: {error}"))
    })?;
    let (sink, receiver) = mpsc::channel(32);
    let emitter = tauri::async_runtime::spawn(emit_token_batches(
        app.clone(),
        Arc::clone(&state.events),
        request.job_id.clone(),
        request.conversation_id.clone(),
        request.message_id.clone(),
        receiver,
    ));
    let result = state
        .engines
        .generate(
            ChatRequest {
                job_id: request.job_id.clone(),
                messages_json,
                max_output_tokens: request.max_output_tokens,
            },
            sink,
        )
        .await;
    emitter
        .await
        .map_err(|error| AppError::Operation(format!("chat token delivery stopped: {error}")))??;

    match result {
        Ok(usage) => {
            state.events.emit(
                &app,
                "chat://usage",
                &request.job_id,
                ChatUsageEvent {
                    job_id: request.job_id.clone(),
                    conversation_id: request.conversation_id.clone(),
                    message_id: request.message_id.clone(),
                    usage: usage.clone(),
                },
            )?;
            emit_state(
                &app,
                &state.events,
                &request,
                ChatGenerationState::Completed,
                None,
            )?;
            clear_event_streams(&state.events, &request.job_id);
            Ok(ChatGenerationResult {
                state: ChatGenerationState::Completed,
                usage: Some(usage),
            })
        }
        Err(AppError::Cancelled(detail)) => {
            emit_state(
                &app,
                &state.events,
                &request,
                ChatGenerationState::Cancelled,
                None,
            )?;
            clear_event_streams(&state.events, &request.job_id);
            tracing::debug!(job_id = %request.job_id, %detail, "chat generation cancelled");
            Ok(ChatGenerationResult {
                state: ChatGenerationState::Cancelled,
                usage: None,
            })
        }
        Err(error) => {
            emit_state(
                &app,
                &state.events,
                &request,
                ChatGenerationState::Failed,
                Some(error.to_string()),
            )?;
            clear_event_streams(&state.events, &request.job_id);
            Err(error.into())
        }
    }
}

#[tauri::command]
pub async fn cancel_chat_generation(
    state: State<'_, AppState>,
    request: CancelChatGenerationRequest,
) -> Result<bool, IpcError> {
    validate_id(&request.job_id, "chat job")?;
    state.engines.cancel_generation(&request.job_id).await?;
    Ok(true)
}

async fn emit_token_batches(
    app: AppHandle,
    events: Arc<EventEmitter>,
    job_id: String,
    conversation_id: String,
    message_id: String,
    mut receiver: mpsc::Receiver<TokenChunk>,
) -> AppResult<()> {
    let mut interval = tokio::time::interval(TOKEN_BATCH_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut pending = String::new();
    let mut last_sequence = 0_u64;
    loop {
        tokio::select! {
            chunk = receiver.recv() => match chunk {
                Some(chunk) => {
                    if chunk.job_id != job_id || chunk.sequence <= last_sequence {
                        continue;
                    }
                    last_sequence = chunk.sequence;
                    pending.push_str(&chunk.text);
                    if pending.len() >= TOKEN_BATCH_BYTES {
                        emit_token_batch(&app, &events, &job_id, &conversation_id, &message_id, &mut pending)?;
                    }
                }
                None => {
                    emit_token_batch(&app, &events, &job_id, &conversation_id, &message_id, &mut pending)?;
                    return Ok(());
                }
            },
            _ = interval.tick() => {
                emit_token_batch(&app, &events, &job_id, &conversation_id, &message_id, &mut pending)?;
            }
        }
    }
}

fn emit_token_batch(
    app: &AppHandle,
    events: &EventEmitter,
    job_id: &str,
    conversation_id: &str,
    message_id: &str,
    pending: &mut String,
) -> AppResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let text = std::mem::take(pending);
    events.emit(
        app,
        "chat://token",
        job_id,
        ChatTokenBatch {
            job_id: job_id.into(),
            conversation_id: conversation_id.into(),
            message_id: message_id.into(),
            text,
        },
    )
}

fn emit_state(
    app: &AppHandle,
    events: &EventEmitter,
    request: &StartChatGenerationRequest,
    generation_state: ChatGenerationState,
    error: Option<String>,
) -> AppResult<()> {
    events.emit(
        app,
        "chat://state-changed",
        &request.job_id,
        ChatStateEvent {
            job_id: request.job_id.clone(),
            conversation_id: request.conversation_id.clone(),
            message_id: request.message_id.clone(),
            state: generation_state,
            error,
        },
    )
}

fn clear_event_streams(events: &EventEmitter, job_id: &str) {
    for event_name in ["chat://token", "chat://usage", "chat://state-changed"] {
        events.clear_stream(event_name, job_id);
    }
}

fn validate_request(request: &StartChatGenerationRequest) -> AppResult<()> {
    validate_id(&request.job_id, "chat job")?;
    validate_id(&request.conversation_id, "conversation")?;
    validate_id(&request.message_id, "message")?;
    validate_id(&request.session_id, "engine session")?;
    if request.messages.is_empty() || request.messages.len() > MAX_MESSAGES {
        return Err(AppError::Operation(format!(
            "chat requires between 1 and {MAX_MESSAGES} messages"
        )));
    }
    if !matches!(
        request.messages.last().map(|message| &message.role),
        Some(ChatRole::User)
    ) {
        return Err(AppError::Operation(
            "the final chat message must have the user role".into(),
        ));
    }
    let mut total = 0_usize;
    for message in &request.messages {
        if message.content.trim().is_empty() || message.content.len() > MAX_MESSAGE_BYTES {
            return Err(AppError::Operation(
                "chat messages must be non-empty and no larger than 256 KiB".into(),
            ));
        }
        total = total.saturating_add(message.content.len());
    }
    if total > MAX_TOTAL_MESSAGE_BYTES {
        return Err(AppError::Operation(
            "the combined chat history exceeds 768 KiB".into(),
        ));
    }
    if !(1..=4_096).contains(&request.max_output_tokens) {
        return Err(AppError::Operation(
            "maximum output tokens must be between 1 and 4096".into(),
        ));
    }
    Ok(())
}

fn validate_id(value: &str, label: &str) -> AppResult<()> {
    if value.trim().is_empty() || value.len() > 128 || value.contains(['\r', '\n', '\0']) {
        return Err(AppError::Operation(format!("the {label} ID is invalid")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_request() -> StartChatGenerationRequest {
        StartChatGenerationRequest {
            job_id: "job-1".into(),
            conversation_id: "conversation-1".into(),
            message_id: "message-1".into(),
            session_id: "session-1".into(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "Hello".into(),
            }],
            max_output_tokens: 512,
        }
    }

    #[test]
    fn validates_bounded_chat_requests() {
        assert!(validate_request(&valid_request()).is_ok());
        let mut request = valid_request();
        request.messages[0].role = ChatRole::Assistant;
        assert!(validate_request(&request).is_err());
        let mut request = valid_request();
        request.messages[0].content = "x".repeat(MAX_MESSAGE_BYTES + 1);
        assert!(validate_request(&request).is_err());
        let mut request = valid_request();
        request.max_output_tokens = 0;
        assert!(validate_request(&request).is_err());
    }
}
