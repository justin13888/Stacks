import PosterCard from "./poster-card";
import Skeleton from "@/components/ui/skeleton";

interface PosterGridItem {
  cameoMediaId: string;
  title: string;
  posterUrl?: string;
  watchStatus?: string;
}

interface PosterGridProps {
  items: PosterGridItem[];
  loading?: boolean;
}

export default function PosterGrid({
  items,
  loading = false,
}: PosterGridProps) {
  return (
    <div
      data-testid="poster-grid"
      className="poster-grid"
      // Responsive: 2 cols mobile, 3 tablet, 5+ desktop
    >
      {/* Poster grid stub */}
      {loading
        ? Array.from({ length: 10 }, (_, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: skeleton placeholders have no identity
            <Skeleton key={i} className="poster-grid-skeleton" />
          ))
        : items.map((item) => (
            <PosterCard
              key={item.cameoMediaId}
              cameoMediaId={item.cameoMediaId}
              title={item.title}
              posterUrl={item.posterUrl}
              watchStatus={item.watchStatus}
            />
          ))}
    </div>
  );
}
