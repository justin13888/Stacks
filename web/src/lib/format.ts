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

// Calendar days. The API's air and watch dates are calendar days
// (`YYYY-MM-DD`, no time or zone), so they are read and shown as local days:
// parsing one with `new Date("2026-10-24")` would read it as UTC midnight,
// which is the day before anywhere west of Greenwich.

/** A `YYYY-MM-DD` calendar day as local midnight, or `null` when malformed. */
export function parseDay(day: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (!match) return null;
  const [, y, m, d] = match;
  const date = new Date(Number(y), Number(m) - 1, Number(d));
  return Number.isNaN(date.getTime()) ? null : date;
}

/** A local date as the `YYYY-MM-DD` day it falls on. */
export function toDay(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** Today, where the user is. */
export function today(now: Date = new Date()): string {
  return toDay(now);
}

/** A calendar day for people, e.g. "Sat, Oct 24". */
export function formatDay(
  day: string,
  options: Intl.DateTimeFormatOptions = {
    weekday: "short",
    month: "short",
    day: "numeric",
  },
): string {
  const date = parseDay(day);
  return date ? date.toLocaleDateString(undefined, options) : day;
}

/** An instant for people, in the user's zone, e.g. "Oct 24, 2026". */
export function formatInstant(instant: string): string {
  const date = new Date(instant);
  if (Number.isNaN(date.getTime())) return instant;
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}
