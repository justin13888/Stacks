import { createFileRoute } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/discover")({
  component: () => (
    <PagePlaceholder
      title="Discover"
      description="Trending movies, shows, anime and dramas, by category."
    />
  ),
});
