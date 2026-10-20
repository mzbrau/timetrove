export function formatGap(ms: number) {
  const totalMins = Math.floor(ms / 60000);
  const hours = Math.floor(totalMins / 60);
  const mins = totalMins % 60;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

/** Duration as `HH:MM`, rounded to the nearest minute (min 1 minute if ms > 0). */
export function formatHm(ms: number) {
  if (!Number.isFinite(ms) || ms < 0) return "00:00";
  if (ms === 0) return "00:00";
  let totalMins = Math.round(ms / 60_000);
  if (totalMins < 1) totalMins = 1;
  const hours = Math.floor(totalMins / 60);
  const mins = totalMins % 60;
  return `${String(hours).padStart(2, "0")}:${String(mins).padStart(2, "0")}`;
}

/** Duration as `HH:MM:SS`, floored (hours may exceed 24). */
export function formatHms(ms: number) {
  if (!Number.isFinite(ms) || ms < 0) return "00:00:00";
  const totalSecs = Math.floor(ms / 1000);
  const hours = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  return `${String(hours).padStart(2, "0")}:${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

/**
 * Duration as decimal hours, floored to whole minutes (matches Rust `format_hhmm`).
 * Up to 2 decimals; trailing zeros trimmed (`3.5`, `3.33`, `3`).
 */
export function formatDecimalHours(ms: number) {
  if (!Number.isFinite(ms) || ms <= 0) return "0";
  const totalMins = Math.floor(ms / 60_000);
  const hours = totalMins / 60;
  const fixed = hours.toFixed(2);
  return fixed.replace(/\.?0+$/, "");
}

/** Normalize to `#rrggbb` for color inputs / swatches; falls back if invalid. */
export function normalizeHexColor(value: string, fallback = "#0f766e"): string {
  const v = value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(v)) return v.toLowerCase();
  if (/^[0-9a-fA-F]{6}$/.test(v)) return `#${v.toLowerCase()}`;
  if (/^#[0-9a-fA-F]{3}$/.test(v)) {
    const [, r, g, b] = v;
    return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
  }
  return fallback;
}
