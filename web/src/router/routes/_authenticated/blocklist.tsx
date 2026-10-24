import { createFileRoute } from "@tanstack/react-router";
import { BlocklistPage } from "@/features/blocklist/blocklist-page";

export const Route = createFileRoute("/_authenticated/blocklist")({
  component: BlocklistPage,
});
