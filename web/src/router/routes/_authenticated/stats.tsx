import { createLazyFileRoute } from "@tanstack/react-router";
import WatchTimeCard from "@/components/stats/watch-time-card";
import GenreChart from "@/components/stats/genre-chart";
import ActivityHeatmap from "@/components/stats/activity-heatmap";
import YearlySummary from "@/components/stats/yearly-summary";

export const Route = createLazyFileRoute("/_authenticated/stats")({
  component: StatsPage,
});

function StatsPage() {
  return (
    <div data-testid="stats-page" className="stats-page">
      {/* Stats dashboard */}
      <WatchTimeCard totalMinutes={0} />
      <GenreChart data={[]} />
      <ActivityHeatmap data={[]} />
      <YearlySummary data={[]} />
    </div>
  );
}
