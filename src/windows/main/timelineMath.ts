/** Nominal 24h in ms (wall-clock label scale); actual days use `dayDurationMs`. */
export const DAY_MS = 24 * 60 * 60 * 1000;

export function localDayStart(day: string): Date {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d, 0, 0, 0, 0);
}

/** Next local calendar midnight after `day` (handles 23h/25h DST days). */
export function localDayEnd(day: string): Date {
  const start = localDayStart(day);
  return new Date(start.getFullYear(), start.getMonth(), start.getDate() + 1, 0, 0, 0, 0);
}

/** Elapsed ms from local midnight to the next local midnight. */
export function dayDurationMs(day: string): number {
  return localDayEnd(day).getTime() - localDayStart(day).getTime();
}

/** Offset of an ISO instant within the local day, clamped to [0, dayDuration]. */
export function msInDay(day: string, iso: string): number {
  const duration = dayDurationMs(day);
  const offset = new Date(iso).getTime() - localDayStart(day).getTime();
  return Math.max(0, Math.min(duration, offset));
}

export function dayMsToIso(day: string, ms: number): string {
  const duration = dayDurationMs(day);
  const clamped = Math.max(0, Math.min(duration, ms));
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

/** Cap continuous fit zoom so a tiny slice does not explode the track. */
export const MAX_FIT_ZOOM = 8;

export type BookedExtent = { startMs: number; endMs: number };

/** Min start / max end of occupied ranges; null if none. */
export function bookedExtent(
  ranges: { startMs: number; endMs: number }[],
): BookedExtent | null {
  let startMs = Infinity;
  let endMs = -Infinity;
  for (const r of ranges) {
    if (r.endMs <= r.startMs) continue;
    startMs = Math.min(startMs, r.startMs);
    endMs = Math.max(endMs, r.endMs);
  }
  if (!Number.isFinite(startMs) || endMs <= startMs) return null;
  return { startMs, endMs };
}

/**
 * Zoom factor so `extent` fills the viewport on a full-day track of `dayLengthMs`.
 * Clamped to [1, MAX_FIT_ZOOM].
 */
export function fitZoom(
  dayLengthMs: number,
  extent: BookedExtent,
  maxZoom: number = MAX_FIT_ZOOM,
): number {
  const span = extent.endMs - extent.startMs;
  if (span <= 0 || dayLengthMs <= 0) return 1;
  return Math.max(1, Math.min(maxZoom, dayLengthMs / span));
}

/** Fraction of track width to scroll so `extent.startMs` sits at the left edge. */
export function fitScrollRatio(
  extent: BookedExtent,
  dayLengthMs: number,
): number {
  if (dayLengthMs <= 0) return 0;
  return Math.max(0, Math.min(1, extent.startMs / dayLengthMs));
}

/** Next discrete zoom level strictly above `zoom`, or null if already at max. */
export function nextZoomLevel(zoom: number): ZoomLevel | null {
  for (const level of ZOOM_LEVELS) {
    if (level > zoom) return level;
  }
  return null;
}

/** Next discrete zoom level strictly below `zoom`, or null if already at min. */
export function prevZoomLevel(zoom: number): ZoomLevel | null {
  for (let i = ZOOM_LEVELS.length - 1; i >= 0; i--) {
    const level = ZOOM_LEVELS[i];
    if (level < zoom) return level;
  }
  return null;
}

/** Label density for continuous or discrete zoom. */
export function hourLabels(zoom: number): number[] {
  if (zoom < 2) return [0, 6, 12, 18, 24];
  return Array.from({ length: 25 }, (_, i) => i);
}

export type Gap = { startMs: number; endMs: number };

/** Neighbor bounds for resizing a slice without entering other occupied ranges. */
export function resizeNeighborBounds(
  ranges: { id: string; startMs: number; endMs: number }[],
  sliceId: string,
  startMs: number,
  endMs: number,
  dayLengthMs: number = DAY_MS,
): { minBound: number; maxBound: number } {
  let minBound = 0;
  let maxBound = dayLengthMs;
  for (const r of ranges) {
    if (r.id === sliceId) continue;
    if (r.endMs <= startMs) minBound = Math.max(minBound, r.endMs);
    if (r.startMs >= endMs) maxBound = Math.min(maxBound, r.startMs);
  }
  return { minBound, maxBound };
}

/** `HH:MM` local clock for an offset within `day`. */
export function formatDayClock(day: string, ms: number): string {
  const duration = dayDurationMs(day);
  const d = new Date(
    localDayStart(day).getTime() + Math.max(0, Math.min(duration, ms)),
  );
  const h = String(d.getHours()).padStart(2, "0");
  const m = String(d.getMinutes()).padStart(2, "0");
  return `${h}:${m}`;
}

/** Build gaps in [0, dayLengthMs] given occupied ranges (startMs, endMs). */
export function gapsFromRanges(
  ranges: { startMs: number; endMs: number }[],
  dayLengthMs: number = DAY_MS,
): Gap[] {
  const sorted = [...ranges]
    .filter((r) => r.endMs > r.startMs)
    .sort((a, b) => a.startMs - b.startMs);
  const gaps: Gap[] = [];
  let cursor = 0;
  for (const r of sorted) {
    const start = Math.max(cursor, 0);
    const end = Math.min(r.startMs, dayLengthMs);
    if (end > start) gaps.push({ startMs: start, endMs: end });
    cursor = Math.max(cursor, r.endMs);
  }
  if (cursor < dayLengthMs) gaps.push({ startMs: cursor, endMs: dayLengthMs });
  return gaps;
}
