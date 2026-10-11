import { describe, expect, it } from "vitest";
import { fromLocalInput, toLocalInput, todayStr } from "./datetime";

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
});
