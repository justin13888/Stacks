import { infiniteQueryOptions, useInfiniteQuery } from "@tanstack/react-query";
import { History as HistoryIcon } from "lucide-react";
import {
  getListEntryEventsQueryKey,
  getListHistoryQueryKey,
  listEntryEvents,
  listHistory,
} from "@/api/library/library";
import type {
  PageEntryEvent,
  PageHistoryEvent,
  TitleDetail,
} from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { LoadMore } from "@/features/library/load-more";
import { HistoryFeed } from "./history-feed";

const PAGE_SIZE = 50;

function historyPagesOptions() {
  return infiniteQueryOptions({
    queryKey: [...getListHistoryQueryKey(), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listHistory({ cursor: pageParam, limit: PAGE_SIZE }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageHistoryEvent) => last.next_cursor ?? undefined,
  });
}

function entryEventsPagesOptions(ref: string) {
  return infiniteQueryOptions({
    queryKey: [...getListEntryEventsQueryKey(ref), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listEntryEvents(ref, { cursor: pageParam, limit: PAGE_SIZE }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageEntryEvent) => last.next_cursor ?? undefined,
  });
}

function FeedSkeleton() {
  return (
    <div aria-hidden className="flex flex-col gap-2">
      {Array.from({ length: 6 }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
        <Skeleton key={i} className="h-14" />
      ))}
    </div>
  );
}

/** Everything the user watched and changed, newest first, with undo. */
export function HistoryPage() {
  const pages = useInfiniteQuery(historyPagesOptions());
  let body: React.ReactNode;
  if (pages.isPending) body = <FeedSkeleton />;
  else if (!pages.data) {
    body = (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  } else {
    const events = pages.data.pages.flatMap((page) => page.items);
    body =
      events.length === 0 ? (
        <EmptyState
          icon={<HistoryIcon />}
          title="No history yet"
          description="Log an episode or change a title's status and it shows up here."
        />
      ) : (
        <>
          <HistoryFeed events={events} showTitle />
          <LoadMore
            hasNextPage={pages.hasNextPage}
            isFetchingNextPage={pages.isFetchingNextPage}
            fetchNextPage={() => pages.fetchNextPage()}
          />
        </>
      );
  }
  return (
    <>
      <PageHeader
        title="History"
        description="Everything you watched and changed, with undo."
      />
      {body}
    </>
  );
}

/** A title's own history, on its page. */
export function TitleHistory({ title }: { title: TitleDetail }) {
  const pages = useInfiniteQuery(entryEventsPagesOptions(title.ref));
  if (pages.isPending) return <Skeleton className="h-24" />;
  if (!pages.data) {
    return (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  }
  const events = pages.data.pages
    .flatMap((page) => page.items)
    .map((event) => ({ title, event }));
  if (events.length === 0) return null;
  return (
    <section aria-labelledby="activity-heading" className="flex flex-col gap-3">
      <h2
        id="activity-heading"
        className="font-display text-lg font-semibold text-fg"
      >
        Activity
      </h2>
      <HistoryFeed events={events} showTitle={false} />
      <LoadMore
        hasNextPage={pages.hasNextPage}
        isFetchingNextPage={pages.isFetchingNextPage}
        fetchNextPage={() => pages.fetchNextPage()}
      />
    </section>
  );
}
