import { createLazyFileRoute } from "@tanstack/react-router";
import PosterGrid from "@/components/media/poster-grid";
import StatusSelector from "@/components/media/status-selector";
import { useMediaFilters } from "@/hooks/use-media-filters";

export const Route = createLazyFileRoute("/_authenticated/library")({
  component: LibraryPage,
});

function LibraryPage() {
  const { filters, updateFilter } = useMediaFilters();
  // status is a string[] in filters; we expose the first selected status as a single value for the selector stub
  const currentStatus = filters.status[0] ?? null;

  const handleStatusChange = (status: string) => {
    updateFilter("status", status ? [status] : []);
  };

  return (
    <div data-testid="library-page" className="library-page">
      {/* Library page — filterable poster grid */}
      <StatusSelector value={currentStatus} onChange={handleStatusChange} />
      <PosterGrid items={[]} loading={false} />
    </div>
  );
}
