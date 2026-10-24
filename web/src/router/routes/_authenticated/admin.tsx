import { createFileRoute } from "@tanstack/react-router";
import { PageNotFound } from "@/components/layout/page";
import { AdminPage } from "@/features/admin/admin-page";

export const Route = createFileRoute("/_authenticated/admin")({
  component: AdminRoute,
});

function AdminRoute() {
  const { user } = Route.useRouteContext();
  // The API enforces the role on every admin operation; this only keeps the
  // section out of reach of accounts that cannot use it.
  if (user.role !== "admin") return <PageNotFound />;
  return <AdminPage selfId={user.id} />;
}
