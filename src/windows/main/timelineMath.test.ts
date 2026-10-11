import { describe, expect, it } from "vitest";
import {
  DAY_MS,
  dayMsToIso,
  formatDayClock,
  gapsFromRanges,
  hourLabels,
  msInDay,
  resizeNeighborBounds,
  snapMs,
  zoomSnapMins,
} from "./timelineMath";

describe("timelineMath", () => {
  it("maps ISO times into day offsets", () => {
    const day = "2026-10-12";
    const noon = new Date(2026, 9, 12, 12, 0, 0, 0).toISOString();
    expect(msInDay(day, noon)).toBe(12 * 60 * 60 * 1000);
    expect(dayMsToIso(day, 0)).toBe(
      new Date(2026, 9, 12, 0, 0, 0, 0).toISOString(),
    );
    expect(msInDay(day, dayMsToIso(day, DAY_MS))).toBe(DAY_MS);
  });

  it("snaps and chooses step from zoom", () => {
    expect(snapMs(7 * 60_000, 5)).toBe(5 * 60_000);
    expect(snapMs(8 * 60_000, 5)).toBe(10 * 60_000);
    expect(zoomSnapMins(1)).toBe(5);
    expect(zoomSnapMins(2)).toBe(5);
    expect(zoomSnapMins(4)).toBe(1);
    expect(zoomSnapMins(8)).toBe(1);
  });

  it("builds gaps around occupied ranges", () => {
    expect(
      gapsFromRanges([
        { startMs: 2 * 60 * 60_000, endMs: 4 * 60 * 60_000 },
        { startMs: 6 * 60 * 60_000, endMs: 7 * 60 * 60_000 },
      ]),
    ).toEqual([
      { startMs: 0, endMs: 2 * 60 * 60_000 },
      { startMs: 4 * 60 * 60_000, endMs: 6 * 60 * 60_000 },
      { startMs: 7 * 60 * 60_000, endMs: DAY_MS },
    ]);
  });

  it("returns sparse labels at 1x and hourly when zoomed", () => {
    expect(hourLabels(1)).toEqual([0, 6, 12, 18, 24]);
    expect(hourLabels(2)).toHaveLength(25);
  });

  it("clamps resize to neighboring occupied ranges", () => {
    expect(
      resizeNeighborBounds(
        [
          { id: "a", startMs: 2 * 60 * 60_000, endMs: 4 * 60 * 60_000 },
          { id: "b", startMs: 6 * 60 * 60_000, endMs: 8 * 60 * 60_000 },
          { id: "c", startMs: 10 * 60 * 60_000, endMs: 12 * 60 * 60_000 },
        ],
        "b",
        6 * 60 * 60_000,
        8 * 60 * 60_000,
      ),
    ).toEqual({
      minBound: 4 * 60 * 60_000,
      maxBound: 10 * 60 * 60_000,
    });
  });

  it("formats a day clock from offset", () => {
    expect(formatDayClock("2026-10-12", 13 * 60 * 60_000 + 5 * 60_000)).toBe(
      "13:05",
    );
  });
});
