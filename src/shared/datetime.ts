const pad2 = (n: number) => String(n).padStart(2, "0");

export function toLocalInput(iso: string) {
  const d = new Date(iso);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}T${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

export function fromLocalInput(value: string) {
  return new Date(value).toISOString();
}

export function todayStr() {
  const d = new Date();
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/** Parse a `YYYY-MM-DD` as a local calendar date (noon to avoid DST edge cases). */
export function parseDay(day: string): Date {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d, 12, 0, 0);
}

export function formatDay(date: Date): string {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

/** Shift a `YYYY-MM-DD` by `delta` calendar days. */
export function addDays(day: string, delta: number): string {
  const date = parseDay(day);
  date.setDate(date.getDate() + delta);
  return formatDay(date);
}

/** Long weekday name for a `YYYY-MM-DD` (e.g. Monday). */
export function weekdayLong(day: string, locale?: string): string {
  return new Intl.DateTimeFormat(locale, { weekday: "long" }).format(parseDay(day));
}

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;

/** Short weekday + locale month/day for a Monday-start week day string. */
export function dayHeaderLabel(day: string, locale?: string): string {
  const date = parseDay(day);
  // getDay(): 0=Sun … 6=Sat → Mon-start index
  const idx = (date.getDay() + 6) % 7;
  const md = new Intl.DateTimeFormat(locale, {
    month: "2-digit",
    day: "2-digit",
  }).format(date);
  return `${WEEKDAYS[idx]} ${md}`;
}

/**
 * ISO week number and ISO week-year for a `YYYY-MM-DD` (typically a Monday week start).
 * ISO weeks: week 1 contains the year's first Thursday; weeks start Monday.
 */
export function isoWeekParts(day: string): { week: number; year: number } {
  const date = parseDay(day);
  // Thursday of this week determines the ISO year
  const thursday = new Date(date);
  thursday.setDate(date.getDate() - ((date.getDay() + 6) % 7) + 3);
  const isoYear = thursday.getFullYear();
  const jan4 = new Date(isoYear, 0, 4, 12, 0, 0);
  const week1Monday = new Date(jan4);
  week1Monday.setDate(jan4.getDate() - ((jan4.getDay() + 6) % 7));
  const diffDays = Math.round(
    (parseDay(formatDay(date)).getTime() - week1Monday.getTime()) / 86_400_000,
  );
  const week = Math.floor(diffDays / 7) + 1;
  return { week, year: isoYear };
}

/** Monday (`YYYY-MM-DD`) of the given ISO week / ISO week-year. */
export function mondayOfIsoWeek(week: number, year: number): string {
  const jan4 = new Date(year, 0, 4, 12, 0, 0);
  const week1Monday = new Date(jan4);
  week1Monday.setDate(jan4.getDate() - ((jan4.getDay() + 6) % 7));
  const monday = new Date(week1Monday);
  monday.setDate(week1Monday.getDate() + (week - 1) * 7);
  return formatDay(monday);
}
