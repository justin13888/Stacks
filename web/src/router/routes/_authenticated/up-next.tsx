import { createFileRoute, redirect } from "@tanstack/react-router";

// Up Next lives at home (`/`), where the navigation points; this keeps the
// named address working.
export const Route = createFileRoute("/_authenticated/up-next")({
  beforeLoad: () => {
    throw redirect({ to: "/", replace: true });
  },
});
