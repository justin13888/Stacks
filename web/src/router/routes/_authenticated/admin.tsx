import { createFileRoute, notFound } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/admin")({
  // The API enforces the role on every admin operation; this only keeps the
  // section out of reach of accounts that cannot use it.
  beforeLoad: ({ context }) => {
    if (context.user.role !== "admin") throw notFound();
  },
  component: () => (
    <PagePlaceholder
      title="Admin"
      description="Accounts and background jobs."
    />
  ),
});
