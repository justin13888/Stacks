import { useQueryClient } from "@tanstack/react-query";
import { Monitor } from "lucide-react";
import { useState } from "react";
import type { Session } from "@/api/schemas";
import {
  getListSessionsQueryKey,
  useDeleteSession,
  useListSessions,
} from "@/api/sessions/sessions";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { formatInstant } from "@/features/calendar/dates";
import { reportFailure } from "@/features/titles/use-tracking";
import { Meta, SettingsSection } from "./section";

/** At most this many sessions are listed; an account rarely has more. */
const SESSION_PAGE = 200;

/**
 * A browser and platform read from a `User-Agent`, e.g. "Firefox on
 * Linux"; the raw string when neither is recognised.
 */
export function describeDevice(userAgent: string | null | undefined): string {
  if (!userAgent) return "Unknown device";
  const browser =
    [
      ["Edg/", "Edge"],
      ["OPR/", "Opera"],
      ["Firefox/", "Firefox"],
      ["Chrome/", "Chrome"],
      ["Safari/", "Safari"],
    ].find(([token]) => userAgent.includes(token as string))?.[1] ?? null;
  const platform =
    [
      ["Android", "Android"],
      ["iPhone", "iOS"],
      ["iPad", "iPadOS"],
      ["Windows", "Windows"],
      ["Mac OS X", "macOS"],
      ["CrOS", "ChromeOS"],
      ["Linux", "Linux"],
    ].find(([token]) => userAgent.includes(token as string))?.[1] ?? null;
  if (browser && platform) return `${browser} on ${platform}`;
  return browser ?? platform ?? userAgent;
}

/** The browsers signed in to this account, each revocable but this one. */
export function SessionsSection() {
  const sessions = useListSessions({ limit: SESSION_PAGE });
  let body: React.ReactNode;
  if (sessions.isPending) body = <Skeleton className="h-20" />;
  else if (sessions.isError)
    body = (
      <ErrorState
        error={sessions.error}
        onRetry={() => void sessions.refetch()}
      />
    );
  else
    body = (
      <ul className="flex flex-col divide-y divide-line">
        {sessions.data.items.map((session) => (
          <SessionRow key={session.id} session={session} />
        ))}
      </ul>
    );
  return (
    <SettingsSection
      title="Sessions"
      description="Browsers signed in to your account. Revoking one signs it out at once."
    >
      {body}
    </SettingsSection>
  );
}

function SessionRow({ session }: { session: Session }) {
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const device = describeDevice(session.user_agent);
  const revoke = useDeleteSession({
    mutation: {
      onSuccess: () => {
        setConfirming(false);
        toast({
          tone: "success",
          title: "Session revoked",
          description: device,
        });
      },
      onError: (error) => {
        setConfirming(false);
        reportFailure(error);
      },
      onSettled: () =>
        queryClient.invalidateQueries({ queryKey: getListSessionsQueryKey() }),
    },
  });
  return (
    <li className="flex flex-wrap items-center gap-3 py-3">
      <Monitor aria-hidden className="size-5 shrink-0 text-fg-muted" />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate font-medium text-fg">
          {device}
          {session.current && (
            <span className="ml-2 rounded-sm bg-accent-soft px-1.5 py-0.5 text-xs text-accent">
              This browser
            </span>
          )}
        </span>
        <Meta>
          Last seen {formatInstant(session.last_seen_at)}
          {session.ip ? ` · ${session.ip}` : ""} · signed in{" "}
          {formatInstant(session.created_at)}
        </Meta>
      </div>
      {!session.current && (
        <Button
          variant="secondary"
          size="sm"
          aria-label={`Revoke ${device}`}
          onClick={() => setConfirming(true)}
        >
          Revoke
        </Button>
      )}
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Revoke this session?"
        description={`${device} is signed out at once and has to sign in again.`}
        confirmLabel="Revoke session"
        pending={revoke.isPending}
        onConfirm={() => revoke.mutate({ id: session.id })}
      />
    </li>
  );
}
