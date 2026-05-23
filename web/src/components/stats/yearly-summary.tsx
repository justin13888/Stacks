import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

interface YearlyEntry {
  year: number;
  count: number;
}

interface YearlySummaryProps {
  data: YearlyEntry[];
}

export default function YearlySummary({ data }: YearlySummaryProps) {
  const sorted = [...data].sort((a, b) => a.year - b.year);

  return (
    <div data-testid="yearly-summary" className="yearly-summary">
      <h3 className="yearly-summary-title">By Year</h3>
      {sorted.length === 0 ? (
        <p className="yearly-summary-empty">No data.</p>
      ) : (
        <ResponsiveContainer width="100%" height={240}>
          <BarChart
            data={sorted}
            margin={{ top: 8, right: 8, left: 8, bottom: 8 }}
          >
            <CartesianGrid
              strokeDasharray="3 3"
              stroke="var(--chart-grid, #4443)"
            />
            <XAxis dataKey="year" />
            <YAxis allowDecimals={false} />
            <Tooltip cursor={{ fillOpacity: 0.1 }} />
            <Bar
              dataKey="count"
              fill="var(--chart-bar, #4f8cff)"
              radius={[4, 4, 0, 0]}
            />
          </BarChart>
        </ResponsiveContainer>
      )}
    </div>
  );
}
