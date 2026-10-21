import { describe, expect, it } from "vitest";
import {
  formatDecimalHours,
  formatGap,
  formatHm,
  formatHms,
  normalizeHexColor,
} from "./format";

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

describe("formatHms", () => {
  it("formats as HH:MM:SS floored", () => {
    expect(formatHms(0)).toBe("00:00:00");
    expect(formatHms(5 * 60_000 + 23_000)).toBe("00:05:23");
    expect(formatHms(5 * 3_600_000 + 23 * 60_000 + 9_000)).toBe("05:23:09");
    expect(formatHms(999)).toBe("00:00:00");
    expect(formatHms(25 * 3_600_000)).toBe("25:00:00");
    expect(formatHms(-1)).toBe("00:00:00");
  });
});

describe("formatDecimalHours", () => {
  it("formats floored minutes as decimal hours", () => {
    expect(formatDecimalHours(0)).toBe("0");
    expect(formatDecimalHours(3.5 * 60 * 60_000)).toBe("3.5");
    expect(formatDecimalHours(3 * 60 * 60_000)).toBe("3");
    expect(formatDecimalHours(3 * 60 * 60_000 + 20 * 60_000)).toBe("3.33");
  });

  it("floors partial minutes like format_hhmm", () => {
    expect(formatDecimalHours(59_999)).toBe("0");
    expect(formatDecimalHours(60_000)).toBe("0.02");
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
