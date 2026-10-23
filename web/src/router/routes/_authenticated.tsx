import { createFileRoute, redirect } from "@tanstack/react-router";
import { AppShell } from "@/components/layout/app-shell";
import { PageNotFound } from "@/components/layout/page";
import { loadSession } from "@/hooks/use-auth";

// Every signed-in page sits under this layout. The guard asks the API for the
// session (the httpOnly cookie is the only credential) and sends anyone
// without one to sign in.
export const Route = createFileRoute("/_authenticated")({
  beforeLoad: async ({ context }) => {
    const session = await loadSession(context.queryClient);
    if (!session) throw redirect({ to: "/login" });
    return { user: session.user };
  },
  component: AppShell,
  // Inside the shell, so navigation stays on screen.
  notFoundComponent: PageNotFound,
});
