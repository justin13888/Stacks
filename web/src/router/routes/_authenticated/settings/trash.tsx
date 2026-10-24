import { createFileRoute } from "@tanstack/react-router";
import { TrashPage } from "@/features/settings/trash-page";

export const Route = createFileRoute("/_authenticated/settings/trash")({
  component: TrashPage,
});
