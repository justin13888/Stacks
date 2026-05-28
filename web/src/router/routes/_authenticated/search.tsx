import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import type { ApiMediaDtoMediaSearchResponse } from "@/api/schemas";
import SearchBar from "@/components/search/search-bar";
import SearchResults from "@/components/search/search-results";
import { useDebouncedValue } from "@/hooks/use-debounced-value";
import { authFetch } from "@/lib/auth";
import { toDisplayItems } from "@/lib/media";

export const Route = createFileRoute("/_authenticated/search")({
  component: SearchPage,
});

function useMediaSearch(q: string) {
  const trimmed = q.trim();
  return useQuery({
    queryKey: ["media-search", trimmed],
    enabled: trimmed.length > 0,
    queryFn: ({ signal }) => {
      const params = new URLSearchParams({ q: trimmed });
      return authFetch<ApiMediaDtoMediaSearchResponse>({
        url: `/api/v1/media/search?${params.toString()}`,
        method: "GET",
        signal,
      });
    },
  });
}

function SearchPage() {
  const [query, setQuery] = useState("");
  const debouncedQuery = useDebouncedValue(query, 300);
  const search = useMediaSearch(debouncedQuery);

  const results = search.data ? toDisplayItems(search.data.items) : [];

  return (
    <div data-testid="search-page" className="search-page">
      {/* Search page — 300ms debounced full-text query */}
      <SearchBar
        value={query}
        onChange={setQuery}
        onSubmit={() => {
          /* submitting on Enter is a no-op; query auto-fires via debounce */
        }}
      />
      <SearchResults results={results} loading={search.isFetching} />
    </div>
  );
}
