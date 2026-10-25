import {
  infiniteQueryOptions,
  useInfiniteQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useState } from "react";
import {
  getListJobsQueryKey,
  getListUsersQueryKey,
  listJobs,
  listUsers,
  useRetryJob,
  useUpdateUser,
} from "@/api/admin/admin";
import {
  type AdminUser,
  type AdminUserPatch,
  type Job,
  JobStatus,
  JobType,
  type PageAdminUser,
  type PageJob,
  type Role,
} from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState, reportFailure } from "@/components/ui/error-state";
import { LoadMore } from "@/components/ui/load-more";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";
import { formatInstant } from "@/lib/format";

const PAGE = 50;

const ROLE_LABELS: Record<Role, string> = { user: "User", admin: "Admin" };

export const JOB_TYPE_LABELS: Record<JobType, string> = {
  import: "Import",
  notification: "Notification",
  metadata_refresh: "Metadata refresh",
};

export const JOB_STATUS_LABELS: Record<JobStatus, string> = {
  pending: "Pending",
  running: "Running",
  completed: "Completed",
  failed: "Failed",
};

/** `/admin`: accounts and the background job queue. */
export function AdminPage({ selfId }: { selfId: string }) {
  return (
    <>
      <PageHeader title="Admin" description="Accounts and background jobs." />
      <Tabs defaultValue="users">
        <TabsList>
          <TabsTrigger value="users">Users</TabsTrigger>
          <TabsTrigger value="jobs">Jobs</TabsTrigger>
        </TabsList>
        <TabsContent value="users">
          <Users selfId={selfId} />
        </TabsContent>
        <TabsContent value="jobs">
          <Jobs />
        </TabsContent>
      </Tabs>
    </>
  );
}

function usersOptions() {
  return infiniteQueryOptions({
    queryKey: [...getListUsersQueryKey(), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listUsers({ cursor: pageParam, limit: PAGE }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageAdminUser) => last.next_cursor ?? undefined,
  });
}

function Users({ selfId }: { selfId: string }) {
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

type Filter<T extends string> = T | "all";

/** Both filters are the server's, so every page holds only matching jobs. */
function jobsOptions(status: JobStatus | undefined, type: JobType | undefined) {
  return infiniteQueryOptions({
    queryKey: [...getListJobsQueryKey({ status, type }), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listJobs({ status, type, cursor: pageParam, limit: PAGE }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageJob) => last.next_cursor ?? undefined,
  });
}

function Jobs() {
  const [status, setStatus] = useState<Filter<JobStatus>>("all");
  const [type, setType] = useState<Filter<JobType>>("all");
  const pages = useInfiniteQuery(
    jobsOptions(
      status === "all" ? undefined : status,
      type === "all" ? undefined : type,
    ),
  );

  let body: React.ReactNode;
  if (pages.isPending) body = <Skeleton className="h-40" />;
  else if (pages.isError)
    body = (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  else {
    const jobs = pages.data.pages.flatMap((page) => page.items);
    body =
      jobs.length === 0 ? (
        <EmptyState
          className="py-8"
          title="No jobs"
          description="No background job matches these filters."
        />
      ) : (
        <ul className="flex flex-col divide-y divide-line rounded-xl border border-line bg-surface">
          {jobs.map((job) => (
            <JobRow key={job.id} job={job} />
          ))}
        </ul>
      );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap gap-3">
        <Select
          label="Status"
          className="w-40"
          value={status}
          options={[
            { value: "all", label: "Any status" },
            ...Object.values(JobStatus).map((value) => ({
              value,
              label: JOB_STATUS_LABELS[value],
            })),
          ]}
          onValueChange={setStatus}
        />
        <Select
          label="Type"
          className="w-48"
          value={type}
          options={[
            { value: "all", label: "Any type" },
            ...Object.values(JobType).map((value) => ({
              value,
              label: JOB_TYPE_LABELS[value],
            })),
          ]}
          onValueChange={setType}
        />
      </div>
      {body}
      {pages.data && (
        <LoadMore
          hasNextPage={pages.hasNextPage}
          isFetchingNextPage={pages.isFetchingNextPage}
          fetchNextPage={() => pages.fetchNextPage()}
        />
      )}
    </div>
  );
}

const STATUS_TONE: Record<JobStatus, string> = {
  pending: "text-fg-muted",
  running: "text-info",
  completed: "text-success",
  failed: "text-danger",
};

function JobRow({ job }: { job: Job }) {
  const queryClient = useQueryClient();
  const retry = useRetryJob({
    mutation: {
      onSuccess: () =>
        toast({
          tone: "success",
          title: "Job queued again",
          description: JOB_TYPE_LABELS[job.type],
        }),
      onError: reportFailure,
      onSettled: () =>
        queryClient.invalidateQueries({ queryKey: getListJobsQueryKey() }),
    },
  });
  const label = `${JOB_TYPE_LABELS[job.type]} job from ${formatInstant(job.created_at)}`;
  return (
    <li className="flex flex-col gap-2 p-3">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="font-medium text-fg">
            {JOB_TYPE_LABELS[job.type]}{" "}
            <span className={cn("text-sm", STATUS_TONE[job.status])}>
              {JOB_STATUS_LABELS[job.status]}
            </span>
          </span>
          <span className="text-xs text-fg-muted">
            Attempt {job.attempts} of {job.max_attempts} · created{" "}
            {formatInstant(job.created_at)}
          </span>
        </div>
        {job.status === "failed" && (
          <Button
            variant="secondary"
            size="sm"
            aria-label={`Retry ${label}`}
            loading={retry.isPending}
            onClick={() => retry.mutate({ id: job.id })}
          >
            Retry
          </Button>
        )}
      </div>
      <details className="text-sm">
        <summary className="cursor-pointer text-fg-muted">Details</summary>
        <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          <dt className="text-fg-muted">Id</dt>
          <dd className="break-all font-mono">{job.id}</dd>
          <dt className="text-fg-muted">Next run</dt>
          <dd>{formatInstant(job.run_at)}</dd>
          {job.started_at && (
            <>
              <dt className="text-fg-muted">Started</dt>
              <dd>{formatInstant(job.started_at)}</dd>
            </>
          )}
          {job.completed_at && (
            <>
              <dt className="text-fg-muted">Ended</dt>
              <dd>{formatInstant(job.completed_at)}</dd>
            </>
          )}
          <dt className="text-fg-muted">Last error</dt>
          <dd className="break-words font-mono whitespace-pre-wrap">
            {job.error ?? "None"}
          </dd>
          <dt className="text-fg-muted">Payload</dt>
          <dd className="text-fg-muted">
            Not shown: a job's payload can carry what only the job should read.
          </dd>
        </dl>
      </details>
    </li>
  );
}
