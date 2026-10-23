import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";
import { describeError } from "@/components/ui/error-state";
import { toast } from "@/components/ui/toast";
import { isUnauthenticated, markSignedOut } from "@/hooks/use-auth";
import { isApiProblem } from "@/lib/api-client";
import { FreshnessTracker } from "@/lib/freshness";

export interface QueryClientHooks {
  /** The session ended (a 401 on any call); re-run the route guards. */
  onSignedOut: () => void;
}

/**
 * The app's query client, with the global failure policy:
 *
 * - a 401 anywhere means the session ended: it is forgotten and the route
 *   guard sends the user to sign in;
 * - a background refetch that fails (the view still shows data) and a failed
 *   mutation raise a toast; a first load that fails is left to the view's
 *   ErrorState;
 * - upstream and rate-limit failures are retried once, after the server's
 *   `Retry-After` when it gave one; client errors are never retried.
 */
export function createQueryClient(hooks: QueryClientHooks): {
  queryClient: QueryClient;
  freshness: FreshnessTracker;
} {
  let queryClient: QueryClient;

  function signedOut() {
    markSignedOut(queryClient);
    hooks.onSignedOut();
  }

  function report(error: unknown) {
    const { title, detail } = describeError(error);
    toast({ tone: "error", title, description: detail });
  }

  queryClient = new QueryClient({
    queryCache: new QueryCache({
      onError: (error, query) => {
        if (isUnauthenticated(error)) return signedOut();
        if (query.state.data !== undefined) report(error);
      },
    }),
    mutationCache: new MutationCache({
      onError: (error, _variables, _context, mutation) => {
        if (isUnauthenticated(error)) return signedOut();
        // A mutation with its own error handling, or a view that renders the
        // failure (`meta.handlesErrors`), reports for itself.
        if (mutation.options.onError || mutation.meta?.handlesErrors) return;
        report(error);
      },
    }),
    defaultOptions: {
      queries: {
        staleTime: 60 * 1000,
        refetchOnWindowFocus: false,
        retry: (failureCount, error) =>
          failureCount < 1 && (!isApiProblem(error) || error.retryable),
        retryDelay: (_attempt, error) => {
          const seconds = isApiProblem(error)
            ? error.retryAfterSeconds
            : undefined;
          return Math.min((seconds ?? 1) * 1000, 10_000);
        },
      },
      mutations: { retry: 0 },
    },
  });

  const freshness = new FreshnessTracker();
  freshness.watch(queryClient.getQueryCache());
  return { queryClient, freshness };
}
