import {
  infiniteQueryOptions,
  useInfiniteQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useState } from "react";
import {
  getListUsersQueryKey,
  listUsers,
  useUpdateUser,
} from "@/api/admin/admin";
import type {
  AdminUser,
  AdminUserPatch,
  PageAdminUser,
  Role,
} from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { ErrorState, reportFailure } from "@/components/ui/error-state";
import { LoadMore } from "@/components/ui/load-more";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { formatInstant } from "@/lib/format";
import { PurgeUser } from "./purge-user";

const PAGE = 50;

const ROLE_LABELS: Record<Role, string> = { user: "User", admin: "Admin" };

function usersOptions() {
  return infiniteQueryOptions({
    queryKey: [...getListUsersQueryKey(), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listUsers({ cursor: pageParam, limit: PAGE }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageAdminUser) => last.next_cursor ?? undefined,
  });
}

/** Admin's Users tab: every account, with its role and activation. */
export function Users({ selfId }: { selfId: string }) {
  const pages = useInfiniteQuery(usersOptions());
  if (pages.isPending) return <Skeleton className="h-40" />;
  if (pages.isError)
    return (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  const users = pages.data.pages.flatMap((page) => page.items);
  return (
    <>
      <ul className="flex flex-col divide-y divide-line rounded-xl border border-line bg-surface">
        {users.map((user) => (
          <UserRow key={user.id} user={user} self={user.id === selfId} />
        ))}
      </ul>
      <LoadMore
        hasNextPage={pages.hasNextPage}
        isFetchingNextPage={pages.isFetchingNextPage}
        fetchNextPage={() => pages.fetchNextPage()}
      />
    </>
  );
}

interface PendingChange {
  patch: AdminUserPatch;
  title: string;
  description: string;
  confirmLabel: string;
}

function UserRow({ user, self }: { user: AdminUser; self: boolean }) {
  const queryClient = useQueryClient();
  const [pending, setPending] = useState<PendingChange | null>(null);
  const update = useUpdateUser({
    mutation: {
      onSuccess: (saved) => {
        setPending(null);
        toast({
          tone: "success",
          title: `Updated ${saved.display_name}`,
        });
      },
      onError: (error) => {
        setPending(null);
        reportFailure(error);
      },
      onSettled: () =>
        queryClient.invalidateQueries({ queryKey: getListUsersQueryKey() }),
    },
  });
  const deactivated = Boolean(user.deactivated_at);
  const name = user.display_name;
  return (
    <li className="flex flex-wrap items-center gap-3 p-3">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate font-medium text-fg">
          {name}
          {self && <span className="ml-2 text-xs text-fg-muted">(you)</span>}
        </span>
        <span className="truncate text-xs text-fg-muted">
          {user.email ?? "No email"} · joined {formatInstant(user.created_at)}
          {deactivated && user.deactivated_at
            ? ` · deactivated ${formatInstant(user.deactivated_at)}`
            : ""}
        </span>
      </div>
      <Select
        label={`Role of ${name}`}
        hideLabel
        className="w-28"
        value={user.role}
        // An admin can't demote or lock out themselves from here.
        disabled={self}
        options={[
          { value: "user", label: ROLE_LABELS.user },
          { value: "admin", label: ROLE_LABELS.admin },
        ]}
        onValueChange={(role) => {
          if (role === user.role) return;
          setPending({
            patch: { role },
            title: `Make ${name} ${role === "admin" ? "an admin" : "a user"}?`,
            description:
              role === "admin"
                ? `${name} will be able to manage every account and the job queue.`
                : `${name} loses access to Admin.`,
            confirmLabel: `Make ${ROLE_LABELS[role].toLowerCase()}`,
          });
        }}
      />
      <Button
        variant={deactivated ? "secondary" : "danger"}
        size="sm"
        disabled={self}
        aria-label={`${deactivated ? "Reactivate" : "Deactivate"} ${name}`}
        onClick={() =>
          setPending(
            deactivated
              ? {
                  patch: { deactivated: false },
                  title: `Reactivate ${name}?`,
                  description: `${name} can sign in again, with all their data as it was.`,
                  confirmLabel: "Reactivate",
                }
              : {
                  patch: { deactivated: true },
                  title: `Deactivate ${name}?`,
                  description: `${name} is signed out everywhere and can't sign in. Their data is kept, and reactivating gives it back.`,
                  confirmLabel: "Deactivate",
                },
          )
        }
      >
        {deactivated ? "Reactivate" : "Deactivate"}
      </Button>
      {/* Only a deactivated account, never your own, can be deleted. */}
      {deactivated && !self && <PurgeUser user={user} />}
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(open) => {
          if (!open) setPending(null);
        }}
        title={pending?.title ?? ""}
        description={pending?.description ?? ""}
        confirmLabel={pending?.confirmLabel ?? "Confirm"}
        tone={pending?.patch.deactivated ? "danger" : "primary"}
        pending={update.isPending}
        onConfirm={() =>
          pending && update.mutate({ id: user.id, data: pending.patch })
        }
      />
    </li>
  );
}
