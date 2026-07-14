import { describe, expect, it } from "vitest";
import { calculateChatMetrics, estimateTokenCount, type ChatMetricMessage } from "./chat-metrics";

function message(
  role: ChatMetricMessage["role"],
  content: string,
  usage: ChatMetricMessage["usage"] = null,
): ChatMetricMessage {
  return { role, content, usage };
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
});
