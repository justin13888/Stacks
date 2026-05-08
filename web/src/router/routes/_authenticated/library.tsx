import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { useApiWatchHistoryHandlersListHistory } from "@/api/watch-history/watch-history";
import PosterGrid from "@/components/media/poster-grid";
import StatusSelector from "@/components/media/status-selector";
import { useMediaFilters } from "@/hooks/use-media-filters";
import { authFetch } from "@/lib/auth";
import { type ApiMedia } from "@/lib/media";

export const Route = createFileRoute("/_authenticated/library")({
  component: LibraryPage,
});

interface HistoryEntry {
  cameo_media_id: string;
}

interface BatchResponse {
  items: unknown[];
}

function LibraryPage() {
  const { filters, updateFilter } = useMediaFilters();
  const currentStatus = filters.status[0] ?? null;

  const handleStatusChange = (status: string) => {
    updateFilter("status", status ? [status] : []);
  };

  const history = useApiWatchHistoryHandlersListHistory();
  const cameoIds = useMemo(() => {
    const ids = new Set<string>();
    for (const item of history.data?.items ?? []) {
      const e = item as HistoryEntry;
      if (e?.cameo_media_id) ids.add(e.cameo_media_id);
    }
    return [...ids].slice(0, 50);
  }, [history.data]);

  const batch = useQuery({
    queryKey: ["media", "batch", cameoIds],
    enabled: cameoIds.length > 0,
    queryFn: () =>
      authFetch<BatchResponse>("/api/v1/media/batch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ cameo_ids: cameoIds }),
      }),
  });

  const items = (batch.data?.items ?? []).map((item) => {
    const m = item as ApiMedia;
    return {
      cameoMediaId: m.cameo_media_id,
      title: m.title ?? "(untitled)",
      posterUrl: m.poster_url ?? undefined,
      watchStatus: m.watch_status ?? undefined,
    };
  });

  const filtered = currentStatus
    ? items.filter((i) => i.watchStatus === currentStatus)
    : items;

  return (
    <div data-testid="library-page" className="library-page">
      <StatusSelector value={currentStatus} onChange={handleStatusChange} />
      <PosterGrid
        items={filtered}
        loading={history.isLoading || batch.isLoading}
      />
    </div>
  );
}
