import { createFileRoute } from "@tanstack/react-router";
import { UpNextPage } from "@/features/library/up-next-page";

// Home is Up Next; `/up-next` is an alias for it.
export const Route = createFileRoute("/_authenticated/")({
  component: UpNextPage,
});
