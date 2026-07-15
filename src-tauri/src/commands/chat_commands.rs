use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tokio::sync::mpsc;

use crate::{
    app_state::AppState,
    context::{
        ChatMessage, ChatRole, ContextWindowReport, RollingContextWindow, CONTEXT_SAFETY_TOKENS,
        ROLLING_WINDOW_STRATEGY,
    },
    conversations::{BeginTurnInput, ConversationMessageState},
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartChatGenerationRequest {
    job_id: String,
    conversation_id: String,
    user_message_id: String,
    message_id: String,
    session_id: String,
    prompt_version_id: Option<String>,
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
pub struct ChatContextEvent {
    job_id: String,
    conversation_id: String,
    message_id: String,
    context: ContextWindowReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGenerationResult {
    state: ChatGenerationState,
    usage: Option<Usage>,
    context: ContextWindowReport,
}

struct ContextWindowPlan {
    messages: Vec<ChatMessage>,
    report: ContextWindowReport,
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

    let model_id = runtime
        .model_id
        .as_deref()
        .ok_or_else(|| AppError::Engine("the ready model session has no model identity".into()))?;
    let context_capacity = runtime.context_size.ok_or_else(|| {
        AppError::Engine("the ready model session has no context capacity".into())
    })?;
    let context_plan = enforce_context_window(
        &state,
        &request.messages,
        context_capacity,
        request.max_output_tokens,
    )
    .await?;
    let user_content = request
        .messages
        .last()
        .map(|message| message.content.as_str())
        .ok_or_else(|| AppError::Operation("the chat request has no user message".into()))?;
    state.conversations.begin_turn(BeginTurnInput {
        conversation_id: &request.conversation_id,
        user_message_id: &request.user_message_id,
        assistant_message_id: &request.message_id,
        job_id: &request.job_id,
        model_id,
        prompt_version_id: request.prompt_version_id.as_deref(),
        user_content,
        max_output_tokens: request.max_output_tokens,
        context_strategy: ROLLING_WINDOW_STRATEGY,
        context: &context_plan.report,
    })?;

    if let Err(error) =
        emit_context(&app, &state.events, &request, &context_plan.report).and_then(|_| {
            emit_state(
                &app,
                &state.events,
                &request,
                ChatGenerationState::Started,
                None,
            )
        })
    {
        let detail = error.to_string();
        let _ = state.conversations.finalize_assistant(
            &request.message_id,
            &request.job_id,
            "",
            ConversationMessageState::Failed,
            None,
            Some(&detail),
        );
        return Err(error.into());
    }
    let messages_json = serde_json::to_string(&context_plan.messages).map_err(|error| {
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
    let delivery = emitter
        .await
        .map_err(|error| AppError::Operation(format!("chat token delivery stopped: {error}")))
        .and_then(|result| result);
    let generated_text = match delivery {
        Ok(content) => content,
        Err(error) => {
            let detail = error.to_string();
            let _ = state.conversations.finalize_assistant(
                &request.message_id,
                &request.job_id,
                "",
                ConversationMessageState::Failed,
                None,
                Some(&detail),
            );
            let _ = emit_state(
                &app,
                &state.events,
                &request,
                ChatGenerationState::Failed,
                Some(detail),
            );
            clear_event_streams(&state.events, &request.job_id);
            return Err(error.into());
        }
    };

    match result {
        Ok(usage) => {
            state.conversations.finalize_assistant(
                &request.message_id,
                &request.job_id,
                &generated_text,
                ConversationMessageState::Complete,
                Some(&usage),
                Some("completed"),
            )?;
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
                context: context_plan.report,
            })
        }
        Err(AppError::Cancelled(detail)) => {
            state.conversations.finalize_assistant(
                &request.message_id,
                &request.job_id,
                &generated_text,
                ConversationMessageState::Cancelled,
                None,
                Some("cancelled_by_user"),
            )?;
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
                context: context_plan.report,
            })
        }
        Err(error) => {
            let detail = error.to_string();
            if let Err(persistence_error) = state.conversations.finalize_assistant(
                &request.message_id,
                &request.job_id,
                &generated_text,
                ConversationMessageState::Failed,
                None,
                Some(&detail),
            ) {
                tracing::error!(
                    job_id = %request.job_id,
                    %persistence_error,
                    "failed to finalize an errored assistant draft"
                );
            }
            emit_state(
                &app,
                &state.events,
                &request,
                ChatGenerationState::Failed,
                Some(detail),
            )?;
            clear_event_streams(&state.events, &request.job_id);
            Err(error.into())
        }
    }
}

async fn enforce_context_window(
    state: &AppState,
    messages: &[ChatMessage],
    context_capacity: u32,
    max_output_tokens: u32,
) -> AppResult<ContextWindowPlan> {
    let input_token_budget = context_capacity
        .checked_sub(max_output_tokens)
        .and_then(|value| value.checked_sub(CONTEXT_SAFETY_TOKENS))
        .ok_or_else(|| {
            AppError::Operation(format!(
                "the {max_output_tokens}-token response reserve leaves no room in the {context_capacity}-token context window; lower the response limit"
            ))
        })?;
    let window = RollingContextWindow::from_messages(messages)?;
    let total_turns = window.history_turn_count();
    let total_history_messages = window.history_message_count();
    let full_messages = window.messages_with_newest_turns(total_turns);
    let full_tokens = count_context_tokens(state, &full_messages).await?;

    let (messages, input_tokens, retained_turns) = if full_tokens <= u64::from(input_token_budget) {
        (full_messages, full_tokens, total_turns)
    } else {
        let mandatory_messages = window.messages_with_newest_turns(0);
        let mandatory_tokens = count_context_tokens(state, &mandatory_messages).await?;
        if mandatory_tokens > u64::from(input_token_budget) {
            return Err(AppError::Operation(format!(
                "the system prompt and current message need {mandatory_tokens} input tokens, but only {input_token_budget} remain after reserving {max_output_tokens} output tokens; lower the response limit or shorten the prompt/message"
            )));
        }

        let mut selected_messages = mandatory_messages;
        let mut selected_tokens = mandatory_tokens;
        let mut retained_turns = 0;
        for candidate_turns in 1..=total_turns {
            let candidate = window.messages_with_newest_turns(candidate_turns);
            let candidate_tokens = count_context_tokens(state, &candidate).await?;
            if candidate_tokens > u64::from(input_token_budget) {
                break;
            }
            selected_messages = candidate;
            selected_tokens = candidate_tokens;
            retained_turns = candidate_turns;
        }
        (selected_messages, selected_tokens, retained_turns)
    };

    let retained_history_messages = window.retained_history_message_count(retained_turns);
    Ok(ContextWindowPlan {
        messages,
        report: ContextWindowReport {
            strategy: ROLLING_WINDOW_STRATEGY.into(),
            context_capacity,
            input_token_budget,
            input_tokens,
            reserved_output_tokens: max_output_tokens,
            safety_tokens: CONTEXT_SAFETY_TOKENS,
            retained_history_messages: retained_history_messages as u32,
            omitted_history_messages: (total_history_messages - retained_history_messages) as u32,
            approximate: false,
        },
    })
}

async fn count_context_tokens(state: &AppState, messages: &[ChatMessage]) -> AppResult<u64> {
    let messages_json = serde_json::to_string(messages).map_err(|error| {
        AppError::Operation(format!(
            "chat messages could not be encoded for token counting: {error}"
        ))
    })?;
    state.engines.count_chat_tokens(&messages_json).await
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
) -> AppResult<String> {
    let mut interval = tokio::time::interval(TOKEN_BATCH_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut pending = String::new();
    let mut complete = String::new();
    let mut last_sequence = 0_u64;
    loop {
        tokio::select! {
            chunk = receiver.recv() => match chunk {
                Some(chunk) => {
                    if chunk.job_id != job_id || chunk.sequence <= last_sequence {
                        continue;
                    }
                    last_sequence = chunk.sequence;
                    complete.push_str(&chunk.text);
                    pending.push_str(&chunk.text);
                    if pending.len() >= TOKEN_BATCH_BYTES {
                        emit_token_batch(&app, &events, &job_id, &conversation_id, &message_id, &mut pending)?;
                    }
                }
                None => {
                    emit_token_batch(&app, &events, &job_id, &conversation_id, &message_id, &mut pending)?;
                    return Ok(complete);
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

fn emit_context(
    app: &AppHandle,
    events: &EventEmitter,
    request: &StartChatGenerationRequest,
    context: &ContextWindowReport,
) -> AppResult<()> {
    events.emit(
        app,
        "chat://context",
        &request.job_id,
        ChatContextEvent {
            job_id: request.job_id.clone(),
            conversation_id: request.conversation_id.clone(),
            message_id: request.message_id.clone(),
            context: context.clone(),
        },
    )
}

fn clear_event_streams(events: &EventEmitter, job_id: &str) {
    for event_name in [
        "chat://token",
        "chat://usage",
        "chat://state-changed",
        "chat://context",
    ] {
        events.clear_stream(event_name, job_id);
    }
}

fn validate_request(request: &StartChatGenerationRequest) -> AppResult<()> {
    validate_id(&request.job_id, "chat job")?;
    validate_id(&request.conversation_id, "conversation")?;
    validate_id(&request.user_message_id, "user message")?;
    validate_id(&request.message_id, "message")?;
    validate_id(&request.session_id, "engine session")?;
    if let Some(prompt_version_id) = &request.prompt_version_id {
        validate_id(prompt_version_id, "prompt version")?;
    }
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
            user_message_id: "user-message-1".into(),
            message_id: "message-1".into(),
            session_id: "session-1".into(),
            prompt_version_id: None,
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
        request.messages.insert(
            0,
            ChatMessage {
                role: ChatRole::System,
                content: "Review precisely.".into(),
            },
        );
        assert!(validate_request(&request).is_ok());
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
