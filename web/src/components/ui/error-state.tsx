import { CloudOff, RotateCw, TriangleAlert } from "lucide-react";
import { isUnauthenticated } from "@/hooks/use-auth";
import { isApiProblem } from "@/lib/api-client";
import { cn } from "@/lib/cn";
import { Button } from "./button";
import { toast } from "./toast";

export interface ErrorStateProps {
  /** Anything a query or render threw; an `ApiProblem` is described best. */
  error: unknown;
  /** Offers a retry button when given. */
  onRetry?: () => void;
  className?: string;
}

interface Described {
  title: string;
  detail: string;
  upstream: boolean;
}

function retryHint(seconds: number | undefined): string {
  if (seconds === undefined) return "Try again in a moment.";
  if (seconds < 60) return `Try again in ${Math.max(1, Math.ceil(seconds))} s.`;
  return `Try again in ${Math.ceil(seconds / 60)} min.`;
}

/** Turns what was thrown into words a user can act on. */
export function describeError(error: unknown): Described {
  if (isApiProblem(error)) {
    switch (error.kind) {
      case "upstream-unavailable":
        return {
          title: "A metadata provider is unavailable",
          detail: `TMDB or AniList did not answer. ${retryHint(error.retryAfterSeconds)}`,
          upstream: true,
        };
      case "upstream-rate-limited":
        return {
          title: "A metadata provider is busy",
          detail: `TMDB or AniList asked us to slow down. ${retryHint(error.retryAfterSeconds)}`,
          upstream: true,
        };
      case "upstream-invalid-response":
        return {
          title: "A metadata provider sent something unexpected",
          detail: "The provider's answer could not be read. Try again later.",
          upstream: true,
        };
      case "upstream-misconfigured":
        return {
          title: "A metadata provider refused the request",
          detail:
            "TMDB or AniList rejected this server's credentials. Retrying will not help; ask the operator to check the provider configuration.",
          upstream: true,
        };
    }
    if (error.status === 429) {
      return {
        title: "Too many requests",
        detail: retryHint(error.retryAfterSeconds),
        upstream: false,
      };
    }
    return {
      title: error.problem.title ?? "Something went wrong",
      detail:
        error.problem.detail ??
        (error.status >= 500
          ? "The server could not complete the request."
          : `The request failed (${error.status}).`),
      upstream: false,
    };
  }
  if (error instanceof TypeError) {
    // fetch rejects with a TypeError when the network is unreachable.
    return {
      title: "You appear to be offline",
      detail: "Check your connection and try again.",
      upstream: false,
    };
  }
  return {
    title: "Something went wrong",
    detail:
      error instanceof Error ? error.message : "An unexpected error occurred.",
    upstream: false,
  };
}

/** A failed write, unless the session ended (the shell handles that). */
export function reportFailure(error: unknown): void {
  if (isUnauthenticated(error)) return;
  const { title, detail } = describeError(error);
  toast({ tone: "error", title, description: detail });
}

/** A failed view: what went wrong, and a retry where one can help. */
export function ErrorState({ error, onRetry, className }: ErrorStateProps) {
  const { title, detail, upstream } = describeError(error);
  const Icon = upstream ? CloudOff : TriangleAlert;
  return (
    <section
      role="alert"
      // Every variant, whatever its title: the e2e screens check finds any
      // error state by this id.
      data-testid="error-state"
      className={cn(
        "mx-auto flex max-w-md flex-col items-center gap-3 px-6 py-12 text-center",
        className,
      )}
    >
      <div
        aria-hidden
        className="mb-1 flex size-12 items-center justify-center rounded-full bg-surface-raised text-danger"
      >
        <Icon className="size-6" />
      </div>
      <h2 className="text-base font-semibold text-fg">{title}</h2>
      <p className="text-sm text-fg-muted">{detail}</p>
      {onRetry && (
        <Button
          variant="secondary"
          size="sm"
          onClick={onRetry}
          className="mt-2"
        >
          <RotateCw aria-hidden className="size-4" />
          Try again
        </Button>
      )}
    </section>
  );
}
