import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

interface GenreChartEntry {
  genre: string;
  count: number;
}

interface GenreChartProps {
  data: GenreChartEntry[];
}

export default function GenreChart({ data }: GenreChartProps) {
  // Show the top 10 genres in descending count order so the chart fits
  const sorted = [...data]
    .sort((a, b) => b.count - a.count)
    .slice(0, 10)
    // Re-reverse so the largest bar sits at the top of the horizontal axis
    .reverse();

  return (
    <div data-testid="genre-chart" className="genre-chart">
      <h3 className="genre-chart-title">Top Genres</h3>
      {sorted.length === 0 ? (
        <p className="genre-chart-empty">No data.</p>
      ) : (
        <ResponsiveContainer
          width="100%"
          height={Math.max(240, sorted.length * 32)}
        >
          <BarChart
            data={sorted}
            layout="vertical"
            margin={{ top: 8, right: 16, left: 8, bottom: 8 }}
          >
            <CartesianGrid
              strokeDasharray="3 3"
              stroke="var(--chart-grid, #4443)"
            />
            <XAxis type="number" allowDecimals={false} />
            <YAxis type="category" dataKey="genre" width={100} />
            <Tooltip cursor={{ fillOpacity: 0.1 }} />
            <Bar
              dataKey="count"
              fill="var(--chart-bar, #4f8cff)"
              radius={[0, 4, 4, 0]}
            />
          </BarChart>
        </ResponsiveContainer>
      )}
    </div>
  );
}
