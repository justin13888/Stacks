import { createFileRoute } from "@tanstack/react-router";
import PosterGrid from "@/components/media/poster-grid";
import StatusSelector from "@/components/media/status-selector";
import { useMediaFilters } from "@/hooks/use-media-filters";

export const Route = createFileRoute("/_authenticated/library")({
  component: LibraryPage,
});

function LibraryPage() {
  const { filters, updateFilter } = useMediaFilters();
  // status is a string[] in filters; show the first selected status as a single value
  const currentStatus = filters.status[0] ?? null;

  const handleStatusChange = (status: string) => {
    updateFilter("status", status ? [status] : []);
  };

  return (
    <div data-testid="library-page" className="library-page">
      {/* Library page — filterable poster grid */}
      <StatusSelector value={currentStatus} onChange={handleStatusChange} />

      {/*
        No backend endpoint yet for listing the user's tracked library.
        Falls back to an empty grid with a clear empty state.
      */}
      <div data-testid="library-empty" className="library-empty">
        <h2 className="library-empty-title">Coming Soon</h2>
        <p className="library-empty-message">
          Track media to populate your library. Open a movie or TV show and set
          a status — it will appear here once the library listing endpoint
          ships.
        </p>
      </div>

      <PosterGrid items={[]} loading={false} />
    </div>
  );
}
