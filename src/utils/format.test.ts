import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("formats binary gigabytes for hardware readouts", () => {
    expect(formatBytes(8 * 1024 ** 3)).toBe("8.0 GB");
  });

  it("does not invent missing telemetry", () => {
    expect(formatBytes(null)).toBe("Not reported");
  });
});
