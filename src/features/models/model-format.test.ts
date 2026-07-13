import { describe, expect, it } from "vitest";
import { formatContextLength, formatParameterCount } from "./model-format";

describe("model metadata formatting", () => {
  it("formats common parameter scales", () => {
    expect(formatParameterCount(7_000_000_000)).toBe("7B params");
    expect(formatParameterCount(550_000_000)).toBe("550M params");
  });

  it("formats context windows without false precision", () => {
    expect(formatContextLength(32_768)).toBe("32K context");
    expect(formatContextLength(512)).toBe("512 context");
  });

  it("omits missing metadata", () => {
    expect(formatParameterCount(null)).toBeNull();
    expect(formatContextLength(undefined)).toBeNull();
  });
});
