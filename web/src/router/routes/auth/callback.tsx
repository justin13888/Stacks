import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { LoaderCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { ErrorState } from "@/components/ui/error-state";
import {
  exchangeAuthorizationCode,
  sessionQueryOptions,
} from "@/hooks/use-auth";

interface CallbackSearch {
  code?: string;
  state?: string;
  error?: string;
  error_description?: string;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

// The identity provider redirects here with `code` and `state` (or `error`).
// They are posted to `POST /sessions`, which verifies them against the
// browser's sign-in cookie and answers with the session cookie (ADR-0004).
export const Route = createFileRoute("/auth/callback")({
  validateSearch: (search: Record<string, unknown>): CallbackSearch => ({
    code: optionalString(search.code),
    state: optionalString(search.state),
    error: optionalString(search.error),
    error_description: optionalString(search.error_description),
  }),
  component: AuthCallback,
});

export function AuthCallback() {
  const search = Route.useSearch();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [failure, setFailure] = useState<unknown>(null);

  const { code, state } = search;
  const providerError = search.error;

  useEffect(() => {
    if (!code || !state) return;
    let cancelled = false;
    exchangeAuthorizationCode(code, state).then(
      async (session) => {
        if (cancelled) return;
        queryClient.setQueryData(sessionQueryOptions.queryKey, session);
        await navigate({ to: "/", replace: true });
      },
      (error: unknown) => {
        if (!cancelled) setFailure(error);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [code, state, navigate, queryClient]);

  const problem =
    failure ??
    (providerError
      ? new Error(
          search.error_description ?? `Sign-in was refused (${providerError}).`,
        )
      : !code || !state
        ? new Error("The sign-in response was incomplete.")
        : null);

  return (
    <main
      data-testid="auth-callback"
      className="flex min-h-dvh flex-col items-center justify-center gap-4 bg-canvas px-4"
    >
      {problem ? (
        <>
          <ErrorState error={problem} />
          <Link
            to="/login"
            className="text-sm font-medium text-accent hover:underline"
          >
            Back to sign in
          </Link>
        </>
      ) : (
        <p
          role="status"
          className="flex items-center gap-2 text-sm text-fg-muted"
        >
          <LoaderCircle aria-hidden className="size-4 animate-spin" />
          Signing you in…
        </p>
      )}
    </main>
  );
}
