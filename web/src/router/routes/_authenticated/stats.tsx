import { createFileRoute } from "@tanstack/react-router";
import { StatsPage, validateStatsSearch } from "@/features/stats/stats-page";

export const Route = createFileRoute("/_authenticated/stats")({
  validateSearch: validateStatsSearch,
  component: StatsRoute,
});

function StatsRoute() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  return (
    <StatsPage
      search={search}
      onYearChange={(year) => void navigate({ search: { year } })}
    />
  );
}
