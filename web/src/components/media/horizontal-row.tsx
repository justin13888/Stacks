import PosterCard from "./poster-card";
import Skeleton from "@/components/ui/skeleton";

interface HorizontalRowItem {
  cameoMediaId: string;
  title: string;
  posterUrl?: string;
}

interface HorizontalRowProps {
  title: string;
  items: HorizontalRowItem[];
  loading?: boolean;
}

export default function HorizontalRow({
  title,
  items,
  loading = false,
}: HorizontalRowProps) {
  return (
    <section data-testid="horizontal-row" className="horizontal-row">
      {/* Horizontally scrollable row with snap points */}
      <h2 className="horizontal-row-title">{title}</h2>
      <div className="horizontal-row-track">
        {loading
          ? Array.from({ length: 6 }, (_, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: skeleton placeholders have no identity
              <Skeleton key={i} className="horizontal-row-skeleton" />
            ))
          : items.map((item) => (
              <PosterCard
                key={item.cameoMediaId}
                cameoMediaId={item.cameoMediaId}
                title={item.title}
                posterUrl={item.posterUrl}
                className="horizontal-row-card"
              />
            ))}
      </div>
    </section>
  );
}
