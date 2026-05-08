import { createFileRoute } from "@tanstack/react-router";
import { useApiStatsHandlersGetStats } from "@/api/stats/stats";
import ActivityHeatmap from "@/components/stats/activity-heatmap";
import GenreChart from "@/components/stats/genre-chart";
import WatchTimeCard from "@/components/stats/watch-time-card";
import YearlySummary from "@/components/stats/yearly-summary";

export const Route = createFileRoute("/_authenticated/stats")({
  component: StatsPage,
});

interface GenreEntry {
  genre: string;
  count: number;
}
interface ActivityEntry {
  month: string;
  count: number;
}
interface YearlyEntry {
  year: number;
  count: number;
}

function StatsPage() {
  const stats = useApiStatsHandlersGetStats();

  if (stats.isLoading) {
    return <div data-testid="stats-page">Loading…</div>;
  }

  const data = stats.data;
  return (
    <div data-testid="stats-page" className="stats-page">
      <WatchTimeCard totalMinutes={data?.total_watch_time_minutes ?? 0} />
      <GenreChart data={(data?.genre_breakdown ?? []) as GenreEntry[]} />
      <ActivityHeatmap
        data={(data?.monthly_heatmap ?? []) as ActivityEntry[]}
      />
      <YearlySummary data={(data?.yearly_counts ?? []) as YearlyEntry[]} />
    </div>
  );
}
