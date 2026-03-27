interface GenreChartEntry {
  genre: string;
  count: number;
}

interface GenreChartProps {
  data: GenreChartEntry[];
}

export default function GenreChart({ data }: GenreChartProps) {
  return (
    <div data-testid="genre-chart" className="genre-chart">
      {/* Genre chart stub — replace with real chart library */}
      <h3 className="genre-chart-title">Genres</h3>
      {data.length === 0 ? (
        <p className="genre-chart-empty">No data.</p>
      ) : (
        <ul className="genre-chart-list">
          {data.map(({ genre, count }) => (
            <li key={genre} className="genre-chart-item">
              <span className="genre-chart-label">{genre}</span>
              <span className="genre-chart-count">{count}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
