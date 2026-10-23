import { createFileRoute } from "@tanstack/react-router";
import { PageNotFound, PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/admin")({
  component: AdminPage,
});

function AdminPage() {
  const { user } = Route.useRouteContext();
  // The API enforces the role on every admin operation; this only keeps the
  // section out of reach of accounts that cannot use it.
  if (user.role !== "admin") return <PageNotFound />;
  return (
    <PagePlaceholder
      title="Admin"
      description="Accounts and background jobs."
    />
  );
}
