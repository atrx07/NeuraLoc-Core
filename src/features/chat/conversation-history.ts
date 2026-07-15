import type {
  ChatMessageInput,
  ChatUsage,
  ContextWindowReport,
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
  context: ContextWindowReport | null;
  terminalReason: string | null;
}

export interface RetryPlan {
  content: string;
  branchThroughMessageId: string | null;
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
      context: message.context,
      terminalReason: message.terminalReason,
    }));
}

export function generationHistory(messages: LocalMessage[]): ChatMessageInput[] {
  return messages
    .filter((message) => message.role === "user" || message.state === "complete")
    .filter((message) => message.content.length > 0)
    .map((message) => ({ role: message.role, content: message.content }));
}

export function retryPlan(messages: LocalMessage[], assistantMessageId: string): RetryPlan | null {
  const assistantIndex = messages.findIndex(
    (message) => message.id === assistantMessageId && message.role === "assistant",
  );
  if (assistantIndex < 0) return null;
  for (let index = assistantIndex - 1; index >= 0; index -= 1) {
    if (messages[index].role === "user") {
      return {
        content: messages[index].content,
        branchThroughMessageId: index > 0 ? messages[index - 1].id : null,
      };
    }
  }
  return null;
}

export function conversationMaxOutputTokens(
  generationSettings: Record<string, unknown>,
  fallback = 1024,
): number {
  const value = generationSettings.maxOutputTokens;
  return typeof value === "number" && Number.isInteger(value) && value >= 1 && value <= 4096
    ? value
    : fallback;
}
