/** Calendar-month pace helpers for Copilot usage vs day-of-month progress. */

export type PaceStatus = "ahead" | "behind" | "onTrack";

export type PaceMonthPoint = {
  day: number;
  /** Expected burn: day / daysInMonth * 100 */
  expected: number;
  /** Actual usage % for that day, or null if no sample / future day */
  usage: number | null;
};

const ON_TRACK_BAND_PP = 3;

/** Days in the local calendar month of `now`. */
export function daysInMonth(now: Date = new Date()): number {
  return new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate();
}

/** Percent of the calendar month elapsed (day-of-month / days-in-month * 100). */
export function monthProgressPct(now: Date = new Date()): number {
  const days = daysInMonth(now);
  if (days <= 0) return 0;
  return (now.getDate() / days) * 100;
}

/** Positive = under budget (ahead); negative = overspending (behind). */
export function paceDelta(usagePct: number, monthPct: number): number {
  return monthPct - usagePct;
}

export function paceStatus(delta: number): PaceStatus {
  if (delta > ON_TRACK_BAND_PP) return "ahead";
  if (delta < -ON_TRACK_BAND_PP) return "behind";
  return "onTrack";
}

export type HistorySample = {
  fetchedAt: string;
  percentUsed: number;
};

/** True when `iso` falls in the same local calendar month as `now`. */
export function isSameLocalMonth(
  iso: string | null | undefined,
  now: Date = new Date(),
): boolean {
  if (!iso) return false;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return false;
  return d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth();
}

/**
 * Recompute percent used against an active limit (e.g. manual override).
 * Falls back to stored percent when limit is missing/non-positive.
 */
export function percentAgainstLimit(
  used: number,
  activeLimit: number,
  storedPercent: number,
): number {
  if (Number.isFinite(activeLimit) && activeLimit > 0 && Number.isFinite(used)) {
    return (used / activeLimit) * 100;
  }
  return storedPercent;
}

/**
 * Build a full-month series: expected diagonal for every day, usage from the
 * last sample per local day (and optional live sample placed on its fetchedAt day).
 * Future days have usage null so the chart line stops at today.
 */
export function buildMonthPaceSeries(
  history: HistorySample[],
  now: Date = new Date(),
  liveSample: HistorySample | null = null,
): PaceMonthPoint[] {
  const year = now.getFullYear();
  const month = now.getMonth();
  const days = daysInMonth(now);
  const today = now.getDate();

  const byDay = new Map<number, number>();
  for (const p of history) {
    const d = new Date(p.fetchedAt);
    if (Number.isNaN(d.getTime())) continue;
    if (d.getFullYear() !== year || d.getMonth() !== month) continue;
    const day = d.getDate();
    if (day < 1 || day > days) continue;
    byDay.set(day, p.percentUsed);
  }

  if (liveSample && Number.isFinite(liveSample.percentUsed)) {
    const d = new Date(liveSample.fetchedAt);
    if (
      !Number.isNaN(d.getTime()) &&
      d.getFullYear() === year &&
      d.getMonth() === month
    ) {
      const day = d.getDate();
      if (day >= 1 && day <= days) {
        byDay.set(day, liveSample.percentUsed);
      }
    }
  }

  const points: PaceMonthPoint[] = [];
  for (let day = 1; day <= days; day++) {
    const expected = (day / days) * 100;
    let usage: number | null = null;
    if (day <= today) {
      const raw = byDay.get(day);
      usage = raw != null && Number.isFinite(raw) ? raw : null;
    }
    points.push({
      day,
      expected: Math.round(expected * 10) / 10,
      usage: usage != null ? Math.round(usage * 10) / 10 : null,
    });
  }
  return points;
}
