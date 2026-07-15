use tauri::State;

use crate::{
    app_state::AppState,
    conversations::{
        ConversationDetail, ConversationIdRequest, ConversationSummary, ListConversationsRequest,
        RenameConversationRequest, SetConversationPinnedRequest,
    },
    errors::IpcError,
};

#[tauri::command]
pub fn list_conversations(
    state: State<'_, AppState>,
    request: ListConversationsRequest,
) -> Result<Vec<ConversationSummary>, IpcError> {
    state.conversations.list(request).map_err(Into::into)
}

#[tauri::command]
pub fn get_conversation(
    state: State<'_, AppState>,
    request: ConversationIdRequest,
) -> Result<ConversationDetail, IpcError> {
    state
        .conversations
        .get(&request.conversation_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn rename_conversation(
    state: State<'_, AppState>,
    request: RenameConversationRequest,
) -> Result<ConversationDetail, IpcError> {
    state.conversations.rename(request).map_err(Into::into)
}

#[tauri::command]
pub fn set_conversation_pinned(
    state: State<'_, AppState>,
    request: SetConversationPinnedRequest,
) -> Result<ConversationDetail, IpcError> {
    state.conversations.set_pinned(request).map_err(Into::into)
}

#[tauri::command]
pub fn delete_conversation(
    state: State<'_, AppState>,
    request: ConversationIdRequest,
) -> Result<(), IpcError> {
    state
        .conversations
        .delete(&request.conversation_id)
        .map_err(Into::into)
}
