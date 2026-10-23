import { createFileRoute } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/settings/")({
  component: () => (
    <PagePlaceholder
      title="Settings"
      description="Preferences, API tokens, notifications, import and backup. The theme is in the account menu."
    />
  ),
});
