import { describe, expect, it } from "vitest";
import { formatGap } from "./format";

describe("formatGap", () => {
  it("formats minutes only under an hour", () => {
    expect(formatGap(5 * 60_000)).toBe("5m");
    expect(formatGap(0)).toBe("0m");
  });

  it("formats hours and minutes", () => {
    expect(formatGap(90 * 60_000)).toBe("1h 30m");
    expect(formatGap(2 * 60 * 60_000)).toBe("2h 0m");
  });
});
