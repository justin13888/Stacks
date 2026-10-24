// The Stats page's charts. Every mark and label takes a design token, so a
// chart follows the active theme (tokens.css) with no colour of its own:
// one series per chart, in the accent hue, with the value printed at the
// end of each bar and a table for assistive technology.

import { type RefObject, useEffect, useRef, useState } from "react";
import { Bar, BarChart, LabelList, Tooltip, XAxis, YAxis } from "recharts";
import { formatYearMonth } from "@/lib/format";

/** The width a chart draws at before (or without) a measurement. */
const FALLBACK_WIDTH = 480;

/**
 * The element's width, following resizes. Until it has been laid out (and
 * where nothing lays it out, as in tests) it is `FALLBACK_WIDTH`.
 */
function useWidth<T extends HTMLElement>(): [RefObject<T | null>, number] {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(FALLBACK_WIDTH);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const measure = () => {
      const next = node.getBoundingClientRect().width;
      if (next > 0) setWidth(Math.round(next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return [ref, width];
}

/** The theme tokens the charts paint with. */
export const CHART_TOKENS = {
  mark: "var(--color-accent)",
  label: "var(--color-fg-muted)",
  value: "var(--color-fg)",
  cursor: "var(--color-surface-raised)",
} as const;

export interface BarDatum {
  label: string;
  value: number;
}

/** Horizontal bars, largest first, each labelled with its value. */
export function HorizontalBars({
  data,
  label,
  unit,
}: {
  data: readonly BarDatum[];
  /** Names the chart and its table. */
  label: string;
  /** What a value counts, e.g. "titles". */
  unit: string;
}) {
  const height = Math.max(120, data.length * 36 + 16);
  const [ref, width] = useWidth<HTMLDivElement>();
  return (
    <figure aria-label={label} className="m-0">
      <div ref={ref} aria-hidden data-chart={label}>
        <BarChart
          width={width}
          height={height}
          data={[...data]}
          layout="vertical"
          margin={{ top: 4, right: 40, bottom: 4, left: 4 }}
          barCategoryGap={8}
        >
          <XAxis type="number" hide />
          <YAxis
            type="category"
            dataKey="label"
            width={110}
            axisLine={false}
            tickLine={false}
            tick={{ fill: CHART_TOKENS.label, fontSize: 12 }}
          />
          <Tooltip
            cursor={{ fill: CHART_TOKENS.cursor }}
            formatter={(value) => [`${value} ${unit}`, ""]}
            separator=""
            contentStyle={{
              background: "var(--color-surface)",
              border: "1px solid var(--color-line)",
              borderRadius: 8,
              color: "var(--color-fg)",
            }}
            labelStyle={{ color: "var(--color-fg)" }}
            itemStyle={{ color: "var(--color-fg)" }}
          />
          <Bar
            dataKey="value"
            fill={CHART_TOKENS.mark}
            radius={[0, 4, 4, 0]}
            maxBarSize={20}
            isAnimationActive={false}
          >
            <LabelList
              dataKey="value"
              position="right"
              fill={CHART_TOKENS.value}
              fontSize={12}
            />
          </Bar>
        </BarChart>
      </div>
      <table className="sr-only">
        <caption>{label}</caption>
        <tbody>
          {data.map((d) => (
            <tr key={d.label}>
              <th scope="row">{d.label}</th>
              <td>
                {d.value} {unit}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}

export interface MonthCell {
  /** `YYYY-MM`. */
  month: string;
  plays: number;
}

/**
 * The year as twelve cells, each shaded by how much was watched that
 * month (one hue, lighter to stronger), with the count printed in it.
 */
export function MonthHeatmap({
  months,
  label,
}: {
  months: readonly MonthCell[];
  label: string;
}) {
  const max = Math.max(1, ...months.map((m) => m.plays));
  return (
    <ol
      aria-label={label}
      className="grid grid-cols-4 gap-2 sm:grid-cols-6 lg:grid-cols-12"
    >
      {months.map(({ month, plays }) => {
        // 12% (a quiet month) to 70% of the accent over the surface, which
        // keeps the text colour readable on every cell in both themes; an
        // empty month is the sunken surface.
        const strength = plays === 0 ? 0 : 12 + Math.round((plays / max) * 58);
        const name = formatYearMonth(month).split(" ")[0] ?? month;
        return (
          <li
            key={month}
            aria-label={`${formatYearMonth(month)}: ${plays} play${plays === 1 ? "" : "s"}`}
            data-strength={strength}
            className="flex aspect-square flex-col items-center justify-center rounded-lg border border-line text-xs"
            style={{
              backgroundColor:
                strength === 0
                  ? "var(--color-surface-sunken)"
                  : `color-mix(in oklch, var(--color-accent) ${strength}%, var(--color-surface))`,
              color: "var(--color-fg)",
            }}
          >
            <span aria-hidden className="font-medium">
              {name}
            </span>
            <span aria-hidden className="tabular-nums opacity-80">
              {plays}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

/** The twelve months of `year`, with the counts the API gave. */
export function monthsOf(
  year: number,
  counts: readonly { month: string; plays: number }[],
): MonthCell[] {
  const byMonth = new Map(counts.map((c) => [c.month, c.plays]));
  return Array.from({ length: 12 }, (_, i) => {
    const month = `${year}-${String(i + 1).padStart(2, "0")}`;
    return { month, plays: byMonth.get(month) ?? 0 };
  });
}
