interface ActivityEntry {
  month: string;
  count: number;
}

interface ActivityHeatmapProps {
  data: ActivityEntry[];
}

export default function ActivityHeatmap({ data }: ActivityHeatmapProps) {
  return (
    <div data-testid="activity-heatmap" className="activity-heatmap">
      {/* Activity heatmap stub */}
      <h3 className="activity-heatmap-title">Activity</h3>
      {data.length === 0 ? (
        <p className="activity-heatmap-empty">No activity data.</p>
      ) : (
        <div className="activity-heatmap-grid">
          {data.map(({ month, count }) => (
            <div
              key={month}
              className="activity-heatmap-cell"
              title={`${month}: ${count}`}
              data-count={count}
            />
          ))}
        </div>
      )}
    </div>
  );
}
