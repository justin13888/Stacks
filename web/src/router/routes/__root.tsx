import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  type ErrorComponentProps,
  Outlet,
} from "@tanstack/react-router";
import { MotionConfig } from "motion/react";
import { PageNotFound } from "@/components/layout/page";
import { ErrorState } from "@/components/ui/error-state";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { FreshnessTracker } from "@/lib/freshness";
import { usePrefersReducedMotion } from "@/lib/motion";

export interface RouterContext {
  queryClient: QueryClient;
  freshness: FreshnessTracker;
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
  errorComponent: RootError,
  notFoundComponent: PageNotFound,
});

function RootLayout() {
  const reducedMotion = usePrefersReducedMotion();
  return (
    // Under prefers-reduced-motion every `motion` transform and layout
    // animation becomes instant; global.css does the same for CSS.
    <MotionConfig reducedMotion={reducedMotion ? "always" : "never"}>
      <TooltipProvider>
        <Outlet />
        <Toaster />
      </TooltipProvider>
    </MotionConfig>
  );
}

/** The global error boundary: any route that throws while rendering lands here. */
export function RootError({ error, reset }: ErrorComponentProps) {
  return (
    <main className="flex min-h-dvh items-center justify-center bg-canvas">
      <ErrorState error={error} onRetry={reset} />
    </main>
  );
}
