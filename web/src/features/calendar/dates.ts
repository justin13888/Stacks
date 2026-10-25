// The calendar's day arithmetic over `YYYY-MM-DD` local days. The day
// primitives the whole app shares (`parseDay`, `toDay`, `today`, `formatDay`,
// `formatInstant`) live in lib/format, which says why days are local.

import { formatDay, parseDay, toDay } from "@/lib/format";

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

/** A month for people, e.g. "October 2026". */
export function formatMonth(day: string): string {
  return formatDay(day, { month: "long", year: "numeric" });
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
