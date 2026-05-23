import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { ApiDiscoverDtoDiscoverResponse } from "@/api/schemas";
import HorizontalRow from "@/components/media/horizontal-row";
import { authFetch } from "@/lib/auth";
import { toDisplayItems } from "@/lib/media";

export const Route = createFileRoute("/_authenticated/")({
  component: HomePage,
});

function useDiscover(category: "trending" | "popular" | "top-rated") {
  return useQuery({
    queryKey: ["discover", category],
    queryFn: ({ signal }) =>
      authFetch<ApiDiscoverDtoDiscoverResponse>({
        url: `/api/v1/discover/${category}`,
        method: "GET",
        signal,
      }),
  });
}

function HomePage() {
  const trending = useDiscover("trending");
  const popular = useDiscover("popular");
  const topRated = useDiscover("top-rated");

  return (
    <div data-testid="home-page" className="home-page">
      {/* Home page — media shelf rows */}
      <HorizontalRow
        title="Trending"
        loading={trending.isLoading}
        items={trending.data ? toDisplayItems(trending.data.items) : []}
      />
      <HorizontalRow
        title="Popular"
        loading={popular.isLoading}
        items={popular.data ? toDisplayItems(popular.data.items) : []}
      />
      <HorizontalRow
        title="Top Rated"
        loading={topRated.isLoading}
        items={topRated.data ? toDisplayItems(topRated.data.items) : []}
      />
    </div>
  );
}
