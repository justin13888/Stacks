import { createFileRoute } from "@tanstack/react-router";
import { DiscoverPage } from "@/features/discover/discover-page";

export const Route = createFileRoute("/_authenticated/discover/")({
  component: DiscoverPage,
});
