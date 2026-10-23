import { createFileRoute } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/lists/")({
  component: () => (
    <PagePlaceholder
      title="Lists"
      description="Your own collections, ordered the way you like."
    />
  ),
});
