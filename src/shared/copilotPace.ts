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

/**
 * Build a full-month series: expected diagonal for every day, usage from the
 * last sample per local day (and optional live today override). Future days
 * have usage null so the chart line stops at today.
 */
export function buildMonthPaceSeries(
  history: HistorySample[],
  now: Date = new Date(),
  liveUsagePct: number | null = null,
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

  if (liveUsagePct != null && Number.isFinite(liveUsagePct)) {
    byDay.set(today, liveUsagePct);
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
      usage:
        usage != null ? Math.round(usage * 10) / 10 : null,
    });
  }
  return points;
}
