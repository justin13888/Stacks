import { createFileRoute } from "@tanstack/react-router";
import { DataPage } from "@/features/settings/data-page";

export const Route = createFileRoute("/_authenticated/settings/data")({
  component: DataPage,
});
