import { describe, expect, it } from "vitest";
import { formatGap, formatHm, normalizeHexColor } from "./format";

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

describe("formatHm", () => {
  it("formats as HH:MM", () => {
    expect(formatHm(0)).toBe("00:00");
    expect(formatHm(90 * 60_000)).toBe("01:30");
    expect(formatHm(2 * 60 * 60_000)).toBe("02:00");
  });

  it("rounds to the nearest minute with a one-minute floor", () => {
    expect(formatHm(29_999)).toBe("00:01");
    expect(formatHm(30_000)).toBe("00:01");
    expect(formatHm(89_999)).toBe("00:01");
    expect(formatHm(90_000)).toBe("00:02");
  });
});

describe("normalizeHexColor", () => {
  it("normalizes hex colors", () => {
    expect(normalizeHexColor("#E4572E")).toBe("#e4572e");
    expect(normalizeHexColor("17BEBB")).toBe("#17bebb");
    expect(normalizeHexColor("#abc")).toBe("#aabbcc");
    expect(normalizeHexColor("nope", "#0f766e")).toBe("#0f766e");
  });
});
