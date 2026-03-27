interface YearlyEntry {
  year: number;
  count: number;
}

interface YearlySummaryProps {
  data: YearlyEntry[];
}

export default function YearlySummary({ data }: YearlySummaryProps) {
  return (
    <div data-testid="yearly-summary" className="yearly-summary">
      {/* Yearly summary stub */}
      <h3 className="yearly-summary-title">By Year</h3>
      {data.length === 0 ? (
        <p className="yearly-summary-empty">No data.</p>
      ) : (
        <ul className="yearly-summary-list">
          {data.map(({ year, count }) => (
            <li key={year} className="yearly-summary-item">
              <span className="yearly-summary-year">{year}</span>
              <span className="yearly-summary-count">{count}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
