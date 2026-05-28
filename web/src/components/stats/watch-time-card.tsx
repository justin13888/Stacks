import { humanizeMinutes } from "@/lib/format";

interface WatchTimeCardProps {
  totalMinutes: number;
}

export default function WatchTimeCard({ totalMinutes }: WatchTimeCardProps) {
  return (
    <div data-testid="watch-time-card" className="watch-time-card">
      <h3 className="watch-time-card-label">Total Watch Time</h3>
      <p className="watch-time-card-value">{humanizeMinutes(totalMinutes)}</p>
      <p className="watch-time-card-subtext">
        across {totalMinutes.toLocaleString()} minutes
      </p>
    </div>
  );
}
