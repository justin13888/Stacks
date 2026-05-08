import { createFileRoute } from "@tanstack/react-router";
import {
  useApiDiscoverHandlersPopular,
  useApiDiscoverHandlersTopRated,
  useApiDiscoverHandlersTrending,
} from "@/api/discover/discover";
import HorizontalRow from "@/components/media/horizontal-row";
import { toCardItem } from "@/lib/media";

export const Route = createFileRoute("/_authenticated/")({
  component: HomePage,
});

function HomePage() {
  const trending = useApiDiscoverHandlersTrending();
  const popular = useApiDiscoverHandlersPopular();
  const topRated = useApiDiscoverHandlersTopRated();

  return (
    <div data-testid="home-page" className="home-page">
      <HorizontalRow
        title="Trending"
        loading={trending.isLoading}
        items={(trending.data?.items ?? []).map(toCardItem)}
      />
      <HorizontalRow
        title="Popular"
        loading={popular.isLoading}
        items={(popular.data?.items ?? []).map(toCardItem)}
      />
      <HorizontalRow
        title="Top Rated"
        loading={topRated.isLoading}
        items={(topRated.data?.items ?? []).map(toCardItem)}
      />
    </div>
  );
}
