import { describe, expect, it } from "vitest";
import {
  buildMonthPaceSeries,
  daysInMonth,
  monthProgressPct,
  paceDelta,
  paceStatus,
} from "./copilotPace";

describe("daysInMonth / monthProgressPct", () => {
  it("uses calendar days in February (non-leap)", () => {
    const feb14 = new Date(2025, 1, 14, 12, 0, 0);
    expect(daysInMonth(feb14)).toBe(28);
    expect(monthProgressPct(feb14)).toBeCloseTo((14 / 28) * 100, 5);
  });

  it("uses 29 days in a leap February", () => {
    const feb14 = new Date(2024, 1, 14, 12, 0, 0);
    expect(daysInMonth(feb14)).toBe(29);
    expect(monthProgressPct(feb14)).toBeCloseTo((14 / 29) * 100, 5);
  });

  it("treats early February as low month progress", () => {
    const feb3 = new Date(2025, 1, 3, 12, 0, 0);
    expect(monthProgressPct(feb3)).toBeCloseTo((3 / 28) * 100, 5);
  });
});

describe("paceDelta / paceStatus", () => {
  it("marks ahead when usage is below month progress", () => {
    // 30% used, ~50% through month → ahead
    const delta = paceDelta(30, 50);
    expect(delta).toBe(20);
    expect(paceStatus(delta)).toBe("ahead");
  });

  it("marks behind when usage is above month progress", () => {
    // 30% used on day 3 of Feb (~10.7% through) → behind
    const monthPct = (3 / 28) * 100;
    const delta = paceDelta(30, monthPct);
    expect(delta).toBeLessThan(0);
    expect(paceStatus(delta)).toBe("behind");
  });

  it("marks onTrack within ±3pp", () => {
    expect(paceStatus(0)).toBe("onTrack");
    expect(paceStatus(3)).toBe("onTrack");
    expect(paceStatus(-3)).toBe("onTrack");
    expect(paceStatus(3.1)).toBe("ahead");
    expect(paceStatus(-3.1)).toBe("behind");
  });
});

describe("buildMonthPaceSeries", () => {
  it("builds expected diagonal and buckets last sample per day", () => {
    const now = new Date(2025, 1, 5, 18, 0, 0); // Feb 5, 2025
    const history = [
      { fetchedAt: new Date(2025, 1, 1, 10, 0, 0).toISOString(), percentUsed: 5 },
      { fetchedAt: new Date(2025, 1, 1, 22, 0, 0).toISOString(), percentUsed: 8 },
      { fetchedAt: new Date(2025, 1, 3, 12, 0, 0).toISOString(), percentUsed: 15 },
      // Prior month — ignored
      { fetchedAt: new Date(2025, 0, 31, 12, 0, 0).toISOString(), percentUsed: 99 },
    ];
    const series = buildMonthPaceSeries(history, now, null);
    expect(series).toHaveLength(28);
    expect(series[0]).toEqual({ day: 1, expected: Math.round((1 / 28) * 1000) / 10, usage: 8 });
    expect(series[2].usage).toBe(15);
    expect(series[1].usage).toBeNull(); // Feb 2 — no sample
    expect(series[4].usage).toBeNull(); // Feb 5 — no sample yet
    expect(series[27].expected).toBe(100);
    expect(series[27].usage).toBeNull(); // future
  });

  it("prefers live usage for today", () => {
    const now = new Date(2025, 1, 5, 18, 0, 0);
    const history = [
      {
        fetchedAt: new Date(2025, 1, 5, 8, 0, 0).toISOString(),
        percentUsed: 20,
      },
    ];
    const series = buildMonthPaceSeries(history, now, 22.5);
    expect(series[4].usage).toBe(22.5);
  });
});
