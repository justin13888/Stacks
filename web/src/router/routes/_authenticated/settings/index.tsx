import { createFileRoute } from "@tanstack/react-router";
import { SettingsPage } from "@/features/settings/settings-page";
import { useCurrentUser } from "@/hooks/use-auth";

export const Route = createFileRoute("/_authenticated/settings/")({
  component: SettingsRoute,
});

function SettingsRoute() {
  const { user: signedIn } = Route.useRouteContext();
  // The session query, not the route context, follows a saved change.
  const user = useCurrentUser() ?? signedIn;
  return <SettingsPage user={user} />;
}
