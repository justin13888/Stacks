// Calendar dates. The API's air dates are calendar days (`YYYY-MM-DD`, no
// time or zone), so they are read and shown as local days: parsing one with
// `new Date("2026-10-24")` would read it as UTC midnight, which is the day
// before anywhere west of Greenwich.

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

/** `day` moved by `days` (negative for earlier), across months and DST. */
export function addDays(day: string, days: number): string {
  const date = parseDay(day);
  if (!date) return day;
  date.setDate(date.getDate() + days);
  return toDay(date);
}

/** The first day of the month `day` is in. */
export function startOfMonth(day: string): string {
  const date = parseDay(day);
  if (!date) return day;
  return toDay(new Date(date.getFullYear(), date.getMonth(), 1));
}

/** The first day of the month `months` after (or before) `day`'s month. */
export function addMonths(day: string, months: number): string {
  const date = parseDay(day);
  if (!date) return day;
  return toDay(new Date(date.getFullYear(), date.getMonth() + months, 1));
}

/**
 * The days a month grid shows: whole weeks from the Monday on or before the
 * 1st to the Sunday on or after the last day (at most 42 days).
 */
export function monthGrid(month: string): string[] {
  const first = parseDay(startOfMonth(month));
  if (!first) return [];
  const last = new Date(first.getFullYear(), first.getMonth() + 1, 0);
  const lead = (first.getDay() + 6) % 7;
  const trail = 6 - ((last.getDay() + 6) % 7);
  const start = toDay(first);
  const total = lead + last.getDate() + trail;
  return Array.from({ length: total }, (_, i) => addDays(start, i - lead));
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

/** A month for people, e.g. "October 2026". */
export function formatMonth(day: string): string {
  return formatDay(day, { month: "long", year: "numeric" });
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

/** Groups `items` by the day `dayOf` gives, in ascending day order. */
export function groupByDay<T>(
  items: readonly T[],
  dayOf: (item: T) => string,
): [string, T[]][] {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const day = dayOf(item);
    const group = groups.get(day);
    if (group) group.push(item);
    else groups.set(day, [item]);
  }
  return [...groups.entries()].sort(([a], [b]) => a.localeCompare(b));
}
