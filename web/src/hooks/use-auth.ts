// The browser session, entirely through the API's httpOnly cookie (ADR-0004).
//
// Nothing here stores a credential: the cookie is set by `createSession`,
// sent by the mutator on every request, and cleared by
// `deleteCurrentSession`. The app only asks the API who it is signed in as.

import {
  type QueryClient,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import type { CurrentSession } from "@/api/schemas";
import {
  createOidcAuthorization,
  createSession,
  deleteCurrentSession,
  getCurrentSession,
  getGetCurrentSessionQueryKey,
} from "@/api/sessions/sessions";
import { isApiProblem } from "@/lib/api-client";

/** Whether a failure means "not signed in" rather than "could not tell". */
export function isUnauthenticated(error: unknown): boolean {
  return isApiProblem(error) && error.status === 401;
}

/**
 * The current session, or `null` when signed out. A 401 is an answer, not an
 * error: it is cached as `null` so the route guard can redirect without
 * retrying. Any other failure (the API is down) stays an error.
 */
export const sessionQueryOptions = queryOptions({
  queryKey: getGetCurrentSessionQueryKey(),
  queryFn: async ({ signal }): Promise<CurrentSession | null> => {
    try {
      return await getCurrentSession({ signal });
    } catch (error) {
      if (isUnauthenticated(error)) return null;
      throw error;
    }
  },
  staleTime: 5 * 60 * 1000,
  retry: false,
});

/** Resolves the session for a route guard, fetching it at most once. */
export function loadSession(
  queryClient: QueryClient,
): Promise<CurrentSession | null> {
  return queryClient.ensureQueryData(sessionQueryOptions);
}

/** Forgets the session locally after it ended on the server. */
export function markSignedOut(queryClient: QueryClient): void {
  queryClient.setQueryData(sessionQueryOptions.queryKey, null);
}

// The authorization code is single-use, and a callback page can mount twice
// (React StrictMode, a fast re-render). Every mount for the same code shares
// one exchange instead of spending the code twice.
const exchanges = new Map<string, Promise<CurrentSession>>();

/** Trades the identity provider's `code` and `state` for a session cookie. */
export function exchangeAuthorizationCode(
  code: string,
  state: string,
): Promise<CurrentSession> {
  const existing = exchanges.get(code);
  if (existing) return existing;
  const pending = createSession({ code, state });
  exchanges.set(code, pending);
  return pending;
}

/** Sends the browser to the identity provider to sign in. */
export function useSignIn() {
  return useMutation({
    mutationFn: () => createOidcAuthorization(),
    onSuccess: ({ authorization_url }) => {
      window.location.assign(authorization_url);
    },
  });
}

/** Ends this browser's session and returns to the sign-in page. */
export function useSignOut() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  return useMutation({
    mutationFn: async () => {
      try {
        await deleteCurrentSession();
      } catch (error) {
        // Already signed out on the server: the local outcome is the same.
        if (!isUnauthenticated(error)) throw error;
      }
    },
    onSuccess: async () => {
      queryClient.clear();
      markSignedOut(queryClient);
      await navigate({ to: "/login" });
    },
  });
}

/** The signed-in user; only call it under the authenticated layout. */
export function useCurrentUser() {
  const { data } = useQuery(sessionQueryOptions);
  return data?.user ?? null;
}
