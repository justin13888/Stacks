import { createFileRoute } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/")({
  component: () => (
    <PagePlaceholder
      title="Up Next"
      description="The next episode of everything you're watching, and what airs this week. Press ⌘K or Ctrl K to find something to watch."
    />
  ),
});
