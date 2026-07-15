import { describe, expect, it } from "vitest";
import type { ConversationMessage, ConversationSummary } from "../../types/domain";
import {
  conversationMaxOutputTokens,
  generationHistory,
  localMessagesFromConversation,
  rememberedConversation,
  retryPlan,
} from "./conversation-history";

const summary: ConversationSummary = {
  id: "conversation-1",
  title: "Persistence",
  modelId: "model-1",
  modelName: "Qwen 4B",
  promptVersionId: null,
  promptName: null,
  promptVersion: null,
  contextStrategy: "full_history",
  pinned: false,
  messageCount: 2,
  sourceConversationId: null,
  branchMessageId: null,
  createdAt: "2026-07-15T00:00:00Z",
  updatedAt: "2026-07-15T00:00:01Z",
};

function message(overrides: Partial<ConversationMessage>): ConversationMessage {
  return {
    id: "message-1",
    conversationId: "conversation-1",
    parentId: null,
    sourceMessageId: null,
    role: "user",
    content: "Hello",
    state: "complete",
    jobId: null,
    tokenCount: null,
    usage: null,
    context: null,
    terminalReason: null,
    position: 1,
    createdAt: "2026-07-15T00:00:00Z",
    updatedAt: "2026-07-15T00:00:00Z",
    ...overrides,
  };
}

describe("conversation history", () => {
  it("restores only an available durable conversation", () => {
    expect(rememberedConversation([summary], "conversation-1")).toEqual(summary);
    expect(rememberedConversation([summary], "missing")).toBeNull();
  });

  it("orders stored messages and maps interrupted drafts explicitly", () => {
    const restored = localMessagesFromConversation([
      message({
        id: "assistant-1",
        role: "assistant",
        content: "Partial output",
        state: "interrupted",
        terminalReason: "application_restarted",
        position: 2,
      }),
      message({ id: "user-1", position: 1 }),
    ]);
    expect(restored.map((item) => item.id)).toEqual(["user-1", "assistant-1"]);
    expect(restored[1].state).toBe("interrupted");
    expect(restored[1].terminalReason).toBe("application_restarted");
  });

  it("excludes non-complete assistant turns from the next inference history", () => {
    const restored = localMessagesFromConversation([
      message({ id: "user-1", position: 1 }),
      message({ id: "assistant-1", role: "assistant", content: "Done", position: 2 }),
      message({ id: "user-2", content: "Again", position: 3 }),
      message({
        id: "assistant-2",
        role: "assistant",
        content: "Partial",
        state: "cancelled",
        position: 4,
      }),
    ]);
    expect(generationHistory(restored)).toEqual([
      { role: "user", content: "Hello" },
      { role: "assistant", content: "Done" },
      { role: "user", content: "Again" },
    ]);
  });

  it("plans first-turn and later retries without mutating the original path", () => {
    const firstTurn = localMessagesFromConversation([
      message({ id: "user-1", position: 1 }),
      message({ id: "assistant-1", role: "assistant", content: "First", position: 2 }),
    ]);
    expect(retryPlan(firstTurn, "assistant-1")).toEqual({
      content: "Hello",
      branchThroughMessageId: null,
    });

    const laterTurn = [
      ...firstTurn,
      { ...firstTurn[0], id: "user-2", content: "Try again" },
      { ...firstTurn[1], id: "assistant-2", content: "Second" },
    ];
    expect(retryPlan(laterTurn, "assistant-2")).toEqual({
      content: "Try again",
      branchThroughMessageId: "assistant-1",
    });
    expect(retryPlan(laterTurn, "user-2")).toBeNull();
  });

  it("restores only bounded persisted output-token settings", () => {
    expect(conversationMaxOutputTokens({ maxOutputTokens: 768 })).toBe(768);
    expect(conversationMaxOutputTokens({ maxOutputTokens: 0 })).toBe(1024);
    expect(conversationMaxOutputTokens({ maxOutputTokens: "768" })).toBe(1024);
  });
});
