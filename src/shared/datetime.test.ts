import { describe, expect, it } from "vitest";
import {
  addDays,
  dayHeaderLabel,
  fromLocalInput,
  isoWeekParts,
  mondayOfIsoWeek,
  toLocalInput,
  todayStr,
  weekdayLong,
} from "./datetime";

describe("datetime helpers", () => {
  it("round-trips local input through ISO", () => {
    const local = "2026-03-15T14:30";
    const iso = fromLocalInput(local);
    expect(iso).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    const back = toLocalInput(iso);
    expect(back).toBe(local);
  });

  it("formats today as YYYY-MM-DD", () => {
    expect(todayStr()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it("labels weekday and locale month/day", () => {
    expect(dayHeaderLabel("2026-10-12", "en-US")).toBe("Mon 10/12");
    expect(dayHeaderLabel("2026-10-18", "en-US")).toBe("Sun 10/18");
    expect(dayHeaderLabel("2026-10-12", "en-GB")).toBe("Mon 12/10");
    expect(dayHeaderLabel("2026-10-18", "en-GB")).toBe("Sun 18/10");
  });

  it("shifts calendar days and formats long weekday", () => {
    expect(addDays("2026-10-12", 1)).toBe("2026-10-13");
    expect(addDays("2026-10-01", -1)).toBe("2026-09-30");
    expect(weekdayLong("2026-10-12", "en-US")).toBe("Monday");
  });

  it("computes ISO week parts for a Monday", () => {
    // 2026-10-12 is Monday of ISO week 42, 2026
    expect(isoWeekParts("2026-10-12")).toEqual({ week: 42, year: 2026 });
  });

  it("round-trips ISO week to Monday", () => {
    const monday = mondayOfIsoWeek(42, 2026);
    expect(monday).toBe("2026-10-12");
    expect(isoWeekParts(monday)).toEqual({ week: 42, year: 2026 });
  });

  it("handles ISO year boundary (week 1)", () => {
    // ISO week 1 of 2020 starts Mon 2019-12-30 (Jan 1 2020 is Wednesday)
    expect(mondayOfIsoWeek(1, 2020)).toBe("2019-12-30");
    expect(isoWeekParts("2019-12-30")).toEqual({ week: 1, year: 2020 });
  });

  it("rejects invalid week 53 via round-trip", () => {
    // 2022 has 52 ISO weeks; week 53 maps into 2023-W01
    const monday = mondayOfIsoWeek(53, 2022);
    const parts = isoWeekParts(monday);
    expect(parts).not.toEqual({ week: 53, year: 2022 });
  });
});
