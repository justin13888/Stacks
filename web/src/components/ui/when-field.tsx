import { motion } from "motion/react";
import { ToggleGroup } from "radix-ui";
import { useEffect, useId, useState } from "react";
import type { Precision, RunDate } from "@/api/schemas";
import { cn } from "@/lib/cn";
import { formatDay, parseDay, today } from "@/lib/format";
import { inputClass } from "./input";
import { Select } from "./select";

// A past date known to the day, the month, or only the year: the API's
// `RunDate`. The field keeps a full calendar day underneath, so switching
// precision back and forth loses nothing; `toRunDate` drops what the chosen
// precision does not know.

export interface WhenValue {
  /** A full `YYYY-MM-DD` day; month and year precision ignore its tail. */
  day: string;
  precision: Precision;
}

/** The earliest year the field accepts. */
export const MIN_YEAR = 1900;

const PRECISIONS: readonly { value: Precision; label: string }[] = [
  { value: "day", label: "Day" },
  { value: "month", label: "Month" },
  { value: "year", label: "Year" },
];

const MONTHS = Array.from({ length: 12 }, (_, i) => ({
  value: String(i + 1).padStart(2, "0"),
  label: new Date(2000, i, 1).toLocaleDateString(undefined, { month: "long" }),
}));

/** Today, known to the day: every dated action's default. */
export function todayWhen(now: Date = new Date()): WhenValue {
  return { day: today(now), precision: "day" };
}

/** The `RunDate` a value stands for: the first day of its month or year. */
export function toRunDate({ day, precision }: WhenValue): RunDate {
  const [year = "", month = ""] = day.split("-");
  const date =
    precision === "year"
      ? `${year}-01-01`
      : precision === "month"
        ? `${year}-${month}-01`
        : day;
  return { date, precision };
}

/**
 * The instant a play logged for `date` carries: noon, where the user is, on
 * its first day. Noon keeps the day the same in every zone a few hours away.
 */
export function playTime(date: RunDate): string {
  const day = parseDay(date.date) ?? new Date();
  day.setHours(12, 0, 0, 0);
  return day.toISOString();
}

/** A run date for people, as precisely as it is known. */
export function formatRunDate(date: RunDate | null | undefined): string | null {
  if (!date) return null;
  switch (date.precision) {
    case "year":
      return date.date.slice(0, 4);
    case "month":
      return formatDay(date.date, { month: "short", year: "numeric" });
    default:
      return formatDay(date.date, {
        month: "short",
        day: "numeric",
        year: "numeric",
      });
  }
}

/** Why `value` cannot be saved, or `null` when it can. */
export function whenError(
  value: WhenValue,
  now: Date = new Date(),
): string | null {
  const day = parseDay(value.day);
  if (!day || day.getFullYear() < MIN_YEAR) {
    return `Choose a date from ${MIN_YEAR} on.`;
  }
  // Compare only what the precision knows: this month is not in the future.
  const width = { day: 10, month: 7, year: 4 }[value.precision];
  if (value.day.slice(0, width) > today(now).slice(0, width)) {
    return "That's in the future.";
  }
  return null;
}

function daysIn(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** `day` with its year or month replaced, its day clamped to fit. */
function withParts(day: string, parts: { year?: number; month?: number }) {
  const [y = "2000", m = "01", d = "01"] = day.split("-");
  const year = parts.year ?? Number(y);
  const month = parts.month ?? Number(m);
  const date = Math.min(Number(d), daysIn(year, month));
  return [
    String(year).padStart(4, "0"),
    String(month).padStart(2, "0"),
    String(date).padStart(2, "0"),
  ].join("-");
}

/**
 * Picks a past date at the precision the user knows it: a calendar day, a
 * month of a year, or a year alone. The precision is a Radix toggle group
 * (arrow keys move between its options); the selected option's highlight
 * slides between them unless motion is reduced.
 */
export function WhenField({
  label,
  value,
  onChange,
  error,
}: {
  label: string;
  value: WhenValue;
  onChange: (value: WhenValue) => void;
  error?: string | null;
}) {
  const id = useId();
  const errorId = `${id}-error`;
  const [y = "", m = "01"] = value.day.split("-");
  // The year as typed, which may be unfinished ("19") on the way to valid.
  const [yearText, setYearText] = useState(y);
  useEffect(() => setYearText(y), [y]);
  const max = today();
  const invalid = error ? true : undefined;
  const describedBy = error ? errorId : undefined;

  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-1.5 text-sm font-medium text-fg">{label}</legend>
      <ToggleGroup.Root
        type="single"
        value={value.precision}
        onValueChange={(next) => {
          const precision = PRECISIONS.find((p) => p.value === next)?.value;
          if (precision) onChange({ ...value, precision });
        }}
        aria-label="How precisely you know it"
        className="inline-flex self-start rounded-lg bg-surface-sunken p-1"
      >
        {PRECISIONS.map((option) => {
          const selected = option.value === value.precision;
          return (
            <ToggleGroup.Item
              key={option.value}
              value={option.value}
              className={cn(
                "relative inline-flex h-8 items-center justify-center rounded-md px-3 text-sm font-medium",
                "transition-colors duration-(--duration-fast) focus-visible:outline-2 focus-visible:outline-ring",
                selected ? "text-fg" : "text-fg-muted hover:text-fg",
              )}
            >
              {selected && (
                <motion.span
                  aria-hidden
                  layoutId={`${id}-precision`}
                  transition={{ type: "spring", bounce: 0.2, duration: 0.3 }}
                  className="absolute inset-0 rounded-md bg-surface shadow-raised"
                />
              )}
              <span className="relative">{option.label}</span>
            </ToggleGroup.Item>
          );
        })}
      </ToggleGroup.Root>
      <div className="flex flex-wrap items-end gap-3">
        {value.precision === "day" ? (
          <div className="flex flex-col gap-1.5">
            <label htmlFor={`${id}-day`} className="text-sm text-fg-muted">
              Date
            </label>
            <input
              id={`${id}-day`}
              type="date"
              value={value.day}
              min={`${MIN_YEAR}-01-01`}
              max={max}
              aria-invalid={invalid}
              aria-describedby={describedBy}
              onChange={(event) => {
                if (event.target.value) {
                  onChange({ ...value, day: event.target.value });
                }
              }}
              className={cn(inputClass, "w-44")}
            />
          </div>
        ) : (
          <>
            {value.precision === "month" && (
              <Select
                label="Month"
                value={m}
                options={MONTHS}
                onValueChange={(month) =>
                  onChange({
                    ...value,
                    day: withParts(value.day, { month: Number(month) }),
                  })
                }
                className="w-40"
              />
            )}
            <div className="flex flex-col gap-1.5">
              <label htmlFor={`${id}-year`} className="text-sm text-fg-muted">
                Year
              </label>
              <input
                id={`${id}-year`}
                type="number"
                inputMode="numeric"
                min={MIN_YEAR}
                max={max.slice(0, 4)}
                value={yearText}
                aria-invalid={invalid}
                aria-describedby={describedBy}
                onChange={(event) => {
                  const text = event.target.value;
                  setYearText(text);
                  if (/^\d{4}$/.test(text)) {
                    onChange({
                      ...value,
                      day: withParts(value.day, { year: Number(text) }),
                    });
                  }
                }}
                onBlur={() => setYearText(y)}
                className={cn(inputClass, "w-28")}
              />
            </div>
          </>
        )}
      </div>
      {error && (
        <p id={errorId} role="alert" className="text-xs text-danger">
          {error}
        </p>
      )}
    </fieldset>
  );
}
