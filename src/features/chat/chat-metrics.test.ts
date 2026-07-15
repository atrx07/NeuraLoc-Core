import { describe, expect, it } from "vitest";
import { calculateChatMetrics, estimateTokenCount, type ChatMetricMessage } from "./chat-metrics";

function message(
  role: ChatMetricMessage["role"],
  content: string,
  usage: ChatMetricMessage["usage"] = null,
): ChatMetricMessage {
  return { role, content, usage, context: null };
}

describe("chat metrics", () => {
  it("uses backend usage for a completed turn", () => {
    const metrics = calculateChatMetrics([
      message("user", "Hello"),
      message("assistant", "Hi", { promptTokens: 80, outputTokens: 20, tokensPerSecond: 25.5 }),
    ], 4_096);

    expect(metrics).toEqual({
      contextTokens: 100,
      contextCapacity: 4_096,
      contextPercent: 2,
      contextApproximate: false,
      outputTokens: 20,
      outputApproximate: false,
      tokensPerSecond: 25.5,
    });
  });

  it("adds an explicitly approximate live tail to the last exact usage", () => {
    const metrics = calculateChatMetrics([
      message("assistant", "Complete", { promptTokens: 80, outputTokens: 20, tokensPerSecond: 12 }),
      message("user", "12345678"),
      message("assistant", "1234"),
    ], 200);

    expect(metrics.contextTokens).toBe(104);
    expect(metrics.contextPercent).toBe(52);
    expect(metrics.contextApproximate).toBe(true);
    expect(metrics.outputTokens).toBe(1);
    expect(metrics.outputApproximate).toBe(true);
    expect(metrics.tokensPerSecond).toBeNull();
  });

  it("clamps capacity display and leaves an empty conversation unmeasured", () => {
    expect(calculateChatMetrics([], 4_096).contextTokens).toBeNull();
    expect(calculateChatMetrics([message("user", "x".repeat(500))], 100).contextPercent).toBe(100);
    expect(estimateTokenCount("")).toBe(0);
  });

  it("includes a selected system prompt before backend usage is available", () => {
    const metrics = calculateChatMetrics([], 4_096, 120);
    expect(metrics.contextTokens).toBe(120);
    expect(metrics.contextApproximate).toBe(true);
    expect(metrics.contextPercent).toBe(3);
  });

  it("uses the exact admitted rolling window after older history is omitted", () => {
    const assistant = message(
      "assistant",
      "Fresh answer",
      { promptTokens: 2800, outputTokens: 120, tokensPerSecond: 18 },
    );
    assistant.context = {
      strategy: "rolling_window",
      contextCapacity: 4096,
      inputTokenBudget: 3040,
      inputTokens: 2800,
      reservedOutputTokens: 1024,
      safetyTokens: 32,
      retainedHistoryMessages: 8,
      omittedHistoryMessages: 6,
      approximate: false,
    };
    const metrics = calculateChatMetrics([assistant], 8192);

    expect(metrics.contextTokens).toBe(2920);
    expect(metrics.contextCapacity).toBe(4096);
    expect(metrics.contextApproximate).toBe(false);
    expect(metrics.contextPercent).toBe(71);
  });

  it("does not reuse a previous context report while the next turn is pending", () => {
    const previousAssistant = message(
      "assistant",
      "Previous answer",
      { promptTokens: 100, outputTokens: 10, tokensPerSecond: 12 },
    );
    previousAssistant.context = {
      strategy: "rolling_window",
      contextCapacity: 4096,
      inputTokenBudget: 3040,
      inputTokens: 100,
      reservedOutputTokens: 1024,
      safetyTokens: 32,
      retainedHistoryMessages: 0,
      omittedHistoryMessages: 0,
      approximate: false,
    };

    const metrics = calculateChatMetrics([
      previousAssistant,
      message("user", "Next question"),
      message("assistant", ""),
    ], 4096);

    expect(metrics.contextTokens).toBeGreaterThan(110);
    expect(metrics.contextApproximate).toBe(true);
  });
});
