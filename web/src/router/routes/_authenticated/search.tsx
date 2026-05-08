import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import SearchBar from "@/components/search/search-bar";
import SearchResults from "@/components/search/search-results";
import { authFetch } from "@/lib/auth";
import { type ApiMedia } from "@/lib/media";

export const Route = createFileRoute("/_authenticated/search")({
  component: SearchPage,
});

interface SearchResponse {
  items: unknown[];
  next_cursor?: string | null;
}

function SearchPage() {
  const [draft, setDraft] = useState("");
  const [query, setQuery] = useState("");

  const search = useQuery({
    queryKey: ["media", "search", query],
    enabled: query.trim().length > 0,
    queryFn: () =>
      authFetch<SearchResponse>(
        `/api/v1/media/search?q=${encodeURIComponent(query)}`,
        { method: "GET" },
      ),
  });

  const handleSubmit = () => {
    setQuery(draft.trim());
  };

  const results = (search.data?.items ?? []).map((item) => {
    const m = item as ApiMedia;
    return {
      cameoMediaId: m.cameo_media_id,
      title: m.title ?? "(untitled)",
      posterUrl: m.poster_url ?? undefined,
    };
  });

  return (
    <div data-testid="search-page" className="search-page">
      <SearchBar value={draft} onChange={setDraft} onSubmit={handleSubmit} />
      <SearchResults results={results} loading={search.isFetching} />
    </div>
  );
}
