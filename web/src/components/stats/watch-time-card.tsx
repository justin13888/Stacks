interface WatchTimeCardProps {
  totalMinutes: number;
}

function formatWatchTime(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  if (days > 0) return `${days}d ${hours % 24}h`;
  if (hours > 0) return `${hours}h ${minutes % 60}m`;
  return `${minutes}m`;
}

export default function WatchTimeCard({ totalMinutes }: WatchTimeCardProps) {
  return (
    <div data-testid="watch-time-card" className="watch-time-card">
      {/* Watch time card stub */}
      <h3 className="watch-time-card-label">Total Watch Time</h3>
      <p className="watch-time-card-value">{formatWatchTime(totalMinutes)}</p>
    </div>
  );
}
