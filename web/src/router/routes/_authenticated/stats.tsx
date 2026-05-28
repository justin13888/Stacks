import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { ApiStatsDtoStatsResponse } from "@/api/schemas";
import ActivityHeatmap from "@/components/stats/activity-heatmap";
import GenreChart from "@/components/stats/genre-chart";
import WatchTimeCard from "@/components/stats/watch-time-card";
import YearlySummary from "@/components/stats/yearly-summary";
import { authFetch } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/stats")({
  component: StatsPage,
});

function StatsPage() {
  const stats = useQuery({
    queryKey: ["stats"],
    queryFn: ({ signal }) =>
      authFetch<ApiStatsDtoStatsResponse>({
        url: "/api/v1/users/me/stats",
        method: "GET",
        signal,
      }),
  });

  if (stats.isLoading) {
    return (
      <div data-testid="stats-page" className="stats-page">
        <p>Loading stats…</p>
      </div>
    );
  }

  if (stats.isError || !stats.data) {
    return (
      <div data-testid="stats-page" className="stats-page">
        <p>Couldn't load stats.</p>
      </div>
    );
  }

  const {
    total_watch_time_minutes,
    genre_breakdown,
    monthly_heatmap,
    yearly_counts,
  } = stats.data;

  return (
    <div data-testid="stats-page" className="stats-page">
      <WatchTimeCard totalMinutes={total_watch_time_minutes} />
      <GenreChart data={genre_breakdown} />
      <ActivityHeatmap data={monthly_heatmap} />
      <YearlySummary data={yearly_counts} />
    </div>
  );
}
