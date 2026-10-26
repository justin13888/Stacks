import {
  infiniteQueryOptions,
  useInfiniteQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useState } from "react";
import { getListJobsQueryKey, listJobs, useRetryJob } from "@/api/admin/admin";
import { type Job, JobStatus, JobType, type PageJob } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState, reportFailure } from "@/components/ui/error-state";
import { LoadMore } from "@/components/ui/load-more";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";
import { formatInstant } from "@/lib/format";
import { Users } from "./users";

const PAGE = 50;

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
