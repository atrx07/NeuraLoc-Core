import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("formats binary gigabytes for hardware readouts", () => {
    expect(formatBytes(8 * 1024 ** 3)).toBe("8.0 GB");
    expect(formatBytes(750 * 1024 ** 2)).toBe("750 MB");
    expect(formatBytes(0)).toBe("0 B");
  });

  it("does not invent missing telemetry", () => {
    expect(formatBytes(null)).toBe("Not reported");
  });
});
