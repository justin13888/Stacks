import { createFileRoute } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/stats")({
  component: () => (
    <PagePlaceholder
      title="Stats"
      description="Your year in review: hours watched, genres and streaks."
    />
  ),
});
