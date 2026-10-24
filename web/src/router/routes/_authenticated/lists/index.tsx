import { createFileRoute } from "@tanstack/react-router";
import { ListsPage } from "@/features/lists/lists-page";

export const Route = createFileRoute("/_authenticated/lists/")({
  component: ListsPage,
});
