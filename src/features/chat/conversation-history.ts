import type {
  ChatMessageInput,
  ChatUsage,
  ConversationMessage,
  ConversationSummary,
} from "../../types/domain";

export type LocalMessageState = "pending" | "streaming" | "complete" | "cancelled" | "error" | "interrupted";

export interface LocalMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  state: LocalMessageState;
  usage: ChatUsage | null;
  terminalReason: string | null;
}

export function rememberedConversation(
  conversations: ConversationSummary[],
  conversationId: string | null,
): ConversationSummary | null {
  if (!conversationId) return null;
  return conversations.find((conversation) => conversation.id === conversationId) ?? null;
}

export function localMessagesFromConversation(messages: ConversationMessage[]): LocalMessage[] {
  return [...messages]
    .sort((left, right) => left.position - right.position || left.id.localeCompare(right.id))
    .map((message) => ({
      id: message.id,
      role: message.role,
      content: message.content,
      state: {
        complete: "complete",
        draft: "pending",
        cancelled: "cancelled",
        failed: "error",
        interrupted: "interrupted",
      }[message.state] as LocalMessageState,
      usage: message.usage,
      terminalReason: message.terminalReason,
    }));
}

export function generationHistory(messages: LocalMessage[]): ChatMessageInput[] {
  return messages
    .filter((message) => message.role === "user" || message.state === "complete")
    .filter((message) => message.content.length > 0)
    .map((message) => ({ role: message.role, content: message.content }));
}
