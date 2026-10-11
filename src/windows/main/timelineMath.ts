export const DAY_MS = 24 * 60 * 60 * 1000;

export function localDayStart(day: string): Date {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d, 0, 0, 0, 0);
}

export function localDayEnd(day: string): Date {
  return new Date(localDayStart(day).getTime() + DAY_MS);
}

/** Offset of an ISO instant within the local day, clamped to [0, DAY_MS]. */
export function msInDay(day: string, iso: string): number {
  const offset = new Date(iso).getTime() - localDayStart(day).getTime();
  return Math.max(0, Math.min(DAY_MS, offset));
}

export function dayMsToIso(day: string, ms: number): string {
  const clamped = Math.max(0, Math.min(DAY_MS, ms));
  return new Date(localDayStart(day).getTime() + clamped).toISOString();
}

export function snapMs(ms: number, stepMins: number): number {
  const step = stepMins * 60_000;
  return Math.round(ms / step) * step;
}

export function zoomSnapMins(zoom: number): number {
  return zoom >= 4 ? 1 : 5;
}

export const ZOOM_LEVELS = [1, 2, 4, 8] as const;
export type ZoomLevel = (typeof ZOOM_LEVELS)[number];

export function hourLabels(zoom: ZoomLevel): number[] {
  if (zoom === 1) return [0, 6, 12, 18, 24];
  return Array.from({ length: 25 }, (_, i) => i);
}

export type Gap = { startMs: number; endMs: number };

/** Neighbor bounds for resizing a slice without entering other occupied ranges. */
export function resizeNeighborBounds(
  ranges: { id: string; startMs: number; endMs: number }[],
  sliceId: string,
  startMs: number,
  endMs: number,
): { minBound: number; maxBound: number } {
  let minBound = 0;
  let maxBound = DAY_MS;
  for (const r of ranges) {
    if (r.id === sliceId) continue;
    if (r.endMs <= startMs) minBound = Math.max(minBound, r.endMs);
    if (r.startMs >= endMs) maxBound = Math.min(maxBound, r.startMs);
  }
  return { minBound, maxBound };
}

/** `HH:MM` local clock for an offset within `day`. */
export function formatDayClock(day: string, ms: number): string {
  const d = new Date(localDayStart(day).getTime() + Math.max(0, Math.min(DAY_MS, ms)));
  const h = String(d.getHours()).padStart(2, "0");
  const m = String(d.getMinutes()).padStart(2, "0");
  return `${h}:${m}`;
}

/** Build gaps in [0, DAY_MS] given sorted occupied ranges (startMs, endMs). */
export function gapsFromRanges(ranges: { startMs: number; endMs: number }[]): Gap[] {
  const sorted = [...ranges]
    .filter((r) => r.endMs > r.startMs)
    .sort((a, b) => a.startMs - b.startMs);
  const gaps: Gap[] = [];
  let cursor = 0;
  for (const r of sorted) {
    const start = Math.max(cursor, 0);
    const end = Math.min(r.startMs, DAY_MS);
    if (end > start) gaps.push({ startMs: start, endMs: end });
    cursor = Math.max(cursor, r.endMs);
  }
  if (cursor < DAY_MS) gaps.push({ startMs: cursor, endMs: DAY_MS });
  return gaps;
}
