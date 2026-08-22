import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { ApiWatchStateDtoLibraryResponse } from "@/api/schemas";
import PosterGrid from "@/components/media/poster-grid";
import StatusSelector from "@/components/media/status-selector";
import { useMediaFilters } from "@/hooks/use-media-filters";
import { authFetch } from "@/lib/auth";
import { toDisplayItems } from "@/lib/media";

export const Route = createFileRoute("/_authenticated/library")({
  component: LibraryPage,
});

function useLibrary(status: string | null, mediaType: string | null) {
  return useQuery({
    queryKey: ["library", status, mediaType],
    queryFn: ({ signal }) => {
      const params = new URLSearchParams();
      if (status) params.set("status", status);
      if (mediaType) params.set("media_type", mediaType);
      const qs = params.toString();
      return authFetch<ApiWatchStateDtoLibraryResponse>({
        url: `/api/v1/users/me/library${qs ? `?${qs}` : ""}`,
        method: "GET",
        signal,
      });
    },
  });
}

function LibraryPage() {
  const { filters, updateFilter } = useMediaFilters();
  // Both filters are single-select here, though the shared filter state models
  // them as arrays for the richer filter panel used elsewhere.
  const currentStatus = filters.status[0] ?? null;
  const currentMediaType = filters.mediaType[0] ?? null;

  const library = useLibrary(currentStatus, currentMediaType);
  const items = library.data ? toDisplayItems(library.data.items) : [];

  const handleStatusChange = (status: string) => {
    updateFilter("status", status ? [status] : []);
  };

  return (
    <div data-testid="library-page" className="library-page">
      {/* Library page — the user's tracked media, filterable by status */}
      <StatusSelector value={currentStatus} onChange={handleStatusChange} />

      {library.isError ? (
        <div data-testid="library-error" className="library-empty">
          <h2 className="library-empty-title">Couldn't load your library</h2>
          <p className="library-empty-message">
            Something went wrong fetching your tracked media. Try again in a
            moment.
          </p>
        </div>
      ) : !library.isFetching && items.length === 0 ? (
        <div data-testid="library-empty" className="library-empty">
          <h2 className="library-empty-title">
            {currentStatus
              ? "Nothing with that status"
              : "Your library is empty"}
          </h2>
          <p className="library-empty-message">
            {currentStatus
              ? "Try a different status filter."
              : "Search for a movie or show and set a status — it will appear here."}
          </p>
        </div>
      ) : null}

      <PosterGrid items={items} loading={library.isFetching} />
    </div>
  );
}
