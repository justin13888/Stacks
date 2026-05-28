/**
 * Display formatting helpers.
 */

/**
 * Humanize a duration in minutes for stat cards.
 *
 * Examples:
 *   - 0     → "0 min"
 *   - 45    → "45 min"
 *   - 60    → "1h"
 *   - 75    → "1h 15m"
 *   - 1440  → "1d"
 *   - 1500  → "1d 1h"
 *   - 4350  → "3d 0h 30m" → "3d 30m" (collapses zero-hour segment)
 */
export function humanizeMinutes(totalMinutes: number): string {
  if (!Number.isFinite(totalMinutes) || totalMinutes <= 0) return "0 min";
  const rounded = Math.round(totalMinutes);
  if (rounded < 60) return `${rounded} min`;

  const minutes = rounded % 60;
  const totalHours = Math.floor(rounded / 60);
  const hours = totalHours % 24;
  const days = Math.floor(totalHours / 24);

  const parts: string[] = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0 && days === 0) parts.push(`${minutes}m`);

  return parts.join(" ") || `${rounded} min`;
}

/** Format `YYYY-MM` as a short month label, e.g. "Jan 2026". */
export function formatYearMonth(yyyyMm: string): string {
  const [yearStr, monthStr] = yyyyMm.split("-");
  const year = Number(yearStr);
  const monthIndex = Number(monthStr) - 1;
  if (
    !Number.isFinite(year) ||
    !Number.isFinite(monthIndex) ||
    monthIndex < 0 ||
    monthIndex > 11
  ) {
    return yyyyMm;
  }
  const date = new Date(Date.UTC(year, monthIndex, 1));
  return date.toLocaleString(undefined, {
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  });
}
