import { createFileRoute } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/history")({
  component: () => (
    <PagePlaceholder
      title="History"
      description="Everything you watched and changed, with undo."
    />
  ),
});
