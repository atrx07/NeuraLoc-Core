use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

pub const ROLLING_WINDOW_STRATEGY: &str = "rolling_window";
pub const CONTEXT_SAFETY_TOKENS: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextWindowReport {
    pub strategy: String,
    pub context_capacity: u32,
    pub input_token_budget: u32,
    pub input_tokens: u64,
    pub reserved_output_tokens: u32,
    pub safety_tokens: u32,
    pub retained_history_messages: u32,
    pub omitted_history_messages: u32,
    pub approximate: bool,
}

pub(crate) struct RollingContextWindow {
    system: Option<ChatMessage>,
    history_turns: Vec<Vec<ChatMessage>>,
    current_user: ChatMessage,
}

impl RollingContextWindow {
    pub fn from_messages(messages: &[ChatMessage]) -> AppResult<Self> {
        let current_user = messages
            .last()
            .filter(|message| message.role == ChatRole::User)
            .cloned()
            .ok_or_else(|| {
                AppError::Operation("the final chat message must have the user role".into())
            })?;
        let body = &messages[..messages.len().saturating_sub(1)];
        let (system, history) = match body.first() {
            Some(message) if message.role == ChatRole::System => {
                (Some(message.clone()), &body[1..])
            }
            _ => (None, body),
        };
        if history
            .iter()
            .any(|message| message.role == ChatRole::System)
        {
            return Err(AppError::Operation(
                "the system prompt must be the first chat message".into(),
            ));
        }

        let mut history_turns: Vec<Vec<ChatMessage>> = Vec::new();
        for message in history {
            match message.role {
                ChatRole::User => history_turns.push(vec![message.clone()]),
                ChatRole::Assistant => {
                    let turn = history_turns.last_mut().ok_or_else(|| {
                        AppError::Operation(
                            "chat history cannot begin with an assistant message".into(),
                        )
                    })?;
                    if turn.iter().any(|item| item.role == ChatRole::Assistant) {
                        return Err(AppError::Operation(
                            "a chat history turn cannot contain multiple assistant messages".into(),
                        ));
                    }
                    turn.push(message.clone());
                }
                ChatRole::System => unreachable!("system roles were rejected above"),
            }
        }

        Ok(Self {
            system,
            history_turns,
            current_user,
        })
    }

    pub fn history_turn_count(&self) -> usize {
        self.history_turns.len()
    }

    pub fn history_message_count(&self) -> usize {
        self.history_turns.iter().map(Vec::len).sum()
    }

    pub fn messages_with_newest_turns(&self, retained_turns: usize) -> Vec<ChatMessage> {
        let retained_turns = retained_turns.min(self.history_turns.len());
        let first_turn = self.history_turns.len() - retained_turns;
        let mut messages = Vec::with_capacity(
            usize::from(self.system.is_some())
                + self.history_turns[first_turn..]
                    .iter()
                    .map(Vec::len)
                    .sum::<usize>()
                + 1,
        );
        if let Some(system) = &self.system {
            messages.push(system.clone());
        }
        for turn in &self.history_turns[first_turn..] {
            messages.extend(turn.iter().cloned());
        }
        messages.push(self.current_user.clone());
        messages
    }

    pub fn retained_history_message_count(&self, retained_turns: usize) -> usize {
        let retained_turns = retained_turns.min(self.history_turns.len());
        self.history_turns[self.history_turns.len() - retained_turns..]
            .iter()
            .map(Vec::len)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: ChatRole, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.into(),
        }
    }

    #[test]
    fn retains_newest_complete_history_turns() {
        let window = RollingContextWindow::from_messages(&[
            message(ChatRole::System, "Be precise."),
            message(ChatRole::User, "First"),
            message(ChatRole::Assistant, "First answer"),
            message(ChatRole::User, "Second"),
            message(ChatRole::Assistant, "Second answer"),
            message(ChatRole::User, "Current"),
        ])
        .unwrap();

        assert_eq!(window.history_turn_count(), 2);
        assert_eq!(window.history_message_count(), 4);
        assert_eq!(window.retained_history_message_count(1), 2);
        assert_eq!(
            window.messages_with_newest_turns(1),
            vec![
                message(ChatRole::System, "Be precise."),
                message(ChatRole::User, "Second"),
                message(ChatRole::Assistant, "Second answer"),
                message(ChatRole::User, "Current"),
            ]
        );
    }

    #[test]
    fn allows_a_user_turn_without_a_completed_assistant() {
        let window = RollingContextWindow::from_messages(&[
            message(ChatRole::User, "Unanswered"),
            message(ChatRole::User, "Current"),
        ])
        .unwrap();

        assert_eq!(window.history_turn_count(), 1);
        assert_eq!(window.retained_history_message_count(1), 1);
    }

    #[test]
    fn rejects_invalid_role_order() {
        assert!(RollingContextWindow::from_messages(&[
            message(ChatRole::Assistant, "Unexpected"),
            message(ChatRole::User, "Current"),
        ])
        .is_err());
        assert!(RollingContextWindow::from_messages(&[
            message(ChatRole::User, "History"),
            message(ChatRole::System, "Too late"),
            message(ChatRole::User, "Current"),
        ])
        .is_err());
        assert!(
            RollingContextWindow::from_messages(&[message(ChatRole::Assistant, "No user")])
                .is_err()
        );
    }
}
