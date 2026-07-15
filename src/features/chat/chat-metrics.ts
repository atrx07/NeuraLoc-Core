import type { ChatUsage, ContextWindowReport } from "../../types/domain";

export interface ChatMetricMessage {
  role: "user" | "assistant";
  content: string;
  usage: ChatUsage | null;
  context: ContextWindowReport | null;
}

export interface ChatMetrics {
  contextTokens: number | null;
  contextCapacity: number | null;
  contextPercent: number | null;
  contextApproximate: boolean;
  outputTokens: number | null;
  outputApproximate: boolean;
  tokensPerSecond: number | null;
}

export function calculateChatMetrics(
  messages: ChatMetricMessage[],
  contextCapacity: number | null,
  systemPromptTokens = 0,
): ChatMetrics {
  const latestAssistant = [...messages].reverse().find((message) => message.role === "assistant");
  const latestContextMessage = latestAssistant?.context ? latestAssistant : undefined;
  const latestContext = latestContextMessage?.context ?? null;
  const latestUsageIndex = findLatestUsageIndex(messages);
  const latestUsage = latestUsageIndex >= 0 ? messages[latestUsageIndex].usage : null;
  const unmeasuredMessages = messages.slice(latestUsageIndex + 1);
  const unmeasuredTokens = estimateTokenCount(
    unmeasuredMessages.map((message) => message.content).join("\n"),
  );
  const windowOutputTokens = latestContextMessage
    ? latestContextMessage.usage?.outputTokens ?? estimateTokenCount(latestContextMessage.content)
    : 0;
  const contextTokens = latestContext
    ? latestContext.inputTokens + windowOutputTokens
    : latestUsage
      ? latestUsage.promptTokens + latestUsage.outputTokens + unmeasuredTokens
      : messages.length > 0 || systemPromptTokens > 0
        ? systemPromptTokens + estimateTokenCount(messages.map((message) => message.content).join("\n"))
        : null;
  const contextApproximate = latestContextMessage
    ? latestContextMessage.usage === null && latestContextMessage.content.length > 0
    : contextTokens !== null && (latestUsage === null || unmeasuredMessages.length > 0);
  const outputTokens = latestAssistant
    ? latestAssistant.usage?.outputTokens ?? estimateTokenCount(latestAssistant.content)
    : null;
  const outputApproximate = latestAssistant !== undefined && latestAssistant.usage === null;
  const effectiveContextCapacity = latestContext?.contextCapacity ?? contextCapacity;
  const contextPercent = contextTokens !== null && effectiveContextCapacity
    ? Math.min(100, Math.round((contextTokens / effectiveContextCapacity) * 100))
    : null;

  return {
    contextTokens,
    contextCapacity: effectiveContextCapacity,
    contextPercent,
    contextApproximate,
    outputTokens,
    outputApproximate,
    tokensPerSecond: latestAssistant?.usage?.tokensPerSecond ?? null,
  };
}

export function estimateTokenCount(text: string): number {
  return text.length === 0 ? 0 : Math.ceil(text.length / 4);
}

function findLatestUsageIndex(messages: ChatMetricMessage[]): number {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (messages[index].usage) return index;
  }
  return -1;
}
