import { createFileRoute } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/library")({
  component: () => (
    <PagePlaceholder
      title="Library"
      description="Everything you track, as a grid, a list or a status board."
    />
  ),
});
