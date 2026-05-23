import { formatYearMonth } from "@/lib/format";

interface ActivityEntry {
  month: string; // ISO YYYY-MM
  count: number;
}

interface ActivityHeatmapProps {
  data: ActivityEntry[];
}

/**
 * Compact 12-month activity heatmap.
 * Cells are colored on a 5-step scale based on the maximum value in the data.
 */
export default function ActivityHeatmap({ data }: ActivityHeatmapProps) {
  // Take the most recent 12 months by ISO sort
  const recent = [...data]
    .sort((a, b) => a.month.localeCompare(b.month))
    .slice(-12);
  const maxCount = recent.reduce((m, e) => Math.max(m, e.count), 0);

  return (
    <div data-testid="activity-heatmap" className="activity-heatmap">
      <h3 className="activity-heatmap-title">Activity (last 12 months)</h3>
      {recent.length === 0 ? (
        <p className="activity-heatmap-empty">No activity data.</p>
      ) : (
        <div
          className="activity-heatmap-grid"
          style={{
            display: "grid",
            gridTemplateColumns: `repeat(${recent.length}, 1fr)`,
            gap: 4,
          }}
        >
          {recent.map((entry) => {
            const intensity = bucketIntensity(entry.count, maxCount);
            return (
              <div key={entry.month} className="activity-heatmap-col">
                <div
                  className={`activity-heatmap-cell activity-heatmap-cell--${intensity}`}
                  title={`${formatYearMonth(entry.month)}: ${entry.count}`}
                  data-count={entry.count}
                  style={{
                    height: 56,
                    borderRadius: 4,
                    background: heatColor(intensity),
                  }}
                />
                <div
                  className="activity-heatmap-label"
                  style={{
                    fontSize: 10,
                    textAlign: "center",
                    marginTop: 4,
                    opacity: 0.7,
                  }}
                >
                  {monthLabel(entry.month)}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

/** Map a count → 0..4 intensity bucket based on the max value. */
function bucketIntensity(count: number, max: number): 0 | 1 | 2 | 3 | 4 {
  if (max === 0 || count <= 0) return 0;
  const pct = count / max;
  if (pct < 0.25) return 1;
  if (pct < 0.5) return 2;
  if (pct < 0.75) return 3;
  return 4;
}

/** Inline color for each intensity step (works on light + dark themes). */
function heatColor(intensity: 0 | 1 | 2 | 3 | 4): string {
  const palette: Record<0 | 1 | 2 | 3 | 4, string> = {
    0: "var(--heat-0, #e5e7eb)",
    1: "var(--heat-1, #93c5fd)",
    2: "var(--heat-2, #60a5fa)",
    3: "var(--heat-3, #3b82f6)",
    4: "var(--heat-4, #1d4ed8)",
  };
  return palette[intensity];
}

function monthLabel(yyyyMm: string): string {
  const [, monthStr] = yyyyMm.split("-");
  if (!monthStr) return yyyyMm;
  const monthIndex = Number(monthStr) - 1;
  const names = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  return names[monthIndex] ?? yyyyMm;
}
