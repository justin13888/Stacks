import { Outlet, createRootRoute } from "@tanstack/react-router";
import { AnimatePresence } from "motion/react";

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout() {
  return (
    <AnimatePresence mode="wait">
      <Outlet />
    </AnimatePresence>
  );
}
