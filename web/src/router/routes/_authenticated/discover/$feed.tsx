import { createFileRoute, notFound } from "@tanstack/react-router";
import { FeedPage } from "@/features/discover/discover-page";
import { isFeed, validateFeedSearch } from "@/features/discover/feeds";

export const Route = createFileRoute("/_authenticated/discover/$feed")({
  validateSearch: validateFeedSearch,
  beforeLoad: ({ params }) => {
    if (!isFeed(params.feed)) throw notFound();
  },
  component: FeedRoute,
});

function FeedRoute() {
  const { feed } = Route.useParams();
  const { kind } = Route.useSearch();
  if (!isFeed(feed)) return null;
  return <FeedPage key={`${feed}:${kind ?? ""}`} feed={feed} kind={kind} />;
}
