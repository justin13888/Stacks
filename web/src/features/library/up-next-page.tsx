import {
  infiniteQueryOptions,
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { CalendarDays, Clapperboard, Play, Sparkles } from "lucide-react";
import {
  getListUpNextQueryKey,
  listUpNext,
  useListLibrary,
  voidPlay,
} from "@/api/library/library";
import type { LibraryEntry, PageUpNextItem, UpNextItem } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Poster } from "@/components/ui/poster";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { formatDay, formatInstant } from "@/features/calendar/dates";
import { episodeLabel } from "@/features/titles/episodes";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { reportFailure } from "@/features/titles/use-tracking";
import { invalidateTracking, logEpisode } from "./entry";
import { LoadMore } from "./load-more";
import { useQuickLog } from "./use-library";

const upNextOptions = infiniteQueryOptions({
  queryKey: [...getListUpNextQueryKey(), "pages"] as const,
  queryFn: ({ pageParam, signal }) =>
    listUpNext({ cursor: pageParam, limit: 30 }, { signal }),
  initialPageParam: undefined as string | undefined,
  getNextPageParam: (last: PageUpNextItem) => last.next_cursor ?? undefined,
});

/** Logs the episode an Up Next card offers, with an undo. */
function useLogUpNext() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (item: UpNextItem) =>
      logEpisode(item.title.ref, {
        season: item.season,
        episode: item.episode,
      }),
    onSuccess: ({ plays }, item) => {
      toast({
        tone: "success",
        title: `Logged ${episodeLabel(item)}`,
        description: item.title.title,
        action: {
          label: "Undo",
          onClick: () => {
            void Promise.all(plays.map((play) => voidPlay(play.id)))
              .catch(reportFailure)
              .finally(() => invalidateTracking(queryClient, item.title.ref));
          },
        },
      });
    },
    onError: reportFailure,
    onSettled: (_data, _error, item) =>
      invalidateTracking(queryClient, item.title.ref),
  });
}

/**
 * Home: the next episode of every show in progress (most recently watched
 * first), each one tap from logged, and the movies waiting on Planning.
 */
export function UpNextPage() {
  const upNext = useInfiniteQuery(upNextOptions);
  return (
    <>
      <PageHeader
        title="Up Next"
        description="Pick up where you left off."
        actions={
          <Link
            to="/calendar"
            className="inline-flex h-10 items-center gap-2 rounded-md border border-line bg-surface px-4 text-sm font-medium text-fg shadow-raised hover:bg-surface-raised focus-visible:outline-2 focus-visible:outline-ring"
          >
            <CalendarDays aria-hidden className="size-4" />
            Calendar
          </Link>
        }
      />
      <div className="flex flex-col gap-10">
        <section
          aria-labelledby="continue-heading"
          className="flex flex-col gap-3"
        >
          <h2
            id="continue-heading"
            className="font-display text-lg font-semibold text-fg"
          >
            Continue watching
          </h2>
          {upNext.isPending ? (
            <ul
              aria-hidden
              className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3"
            >
              {Array.from({ length: 6 }, (_, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
                <Skeleton key={i} className="h-28" />
              ))}
            </ul>
          ) : upNext.isError ? (
            <ErrorState
              error={upNext.error}
              onRetry={() => void upNext.refetch()}
            />
          ) : (
            <UpNextList
              items={upNext.data.pages.flatMap((page) => page.items)}
              hasNextPage={upNext.hasNextPage}
              isFetchingNextPage={upNext.isFetchingNextPage}
              fetchNextPage={() => upNext.fetchNextPage()}
            />
          )}
        </section>
        <PlannedMovies />
      </div>
    </>
  );
}

function UpNextList({
  items,
  ...paging
}: {
  items: readonly UpNextItem[];
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchNextPage: () => unknown;
}) {
  if (items.length === 0) {
    return (
      <EmptyState
        icon={<Sparkles />}
        title="Nothing in progress"
        description="Start a show and its next episode shows up here. Press ⌘K or Ctrl K to find one."
      />
    );
  }
  return (
    <>
      <ul className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {items.map((item) => (
          <li key={item.title.ref}>
            <UpNextCard item={item} />
          </li>
        ))}
      </ul>
      <LoadMore {...paging} />
    </>
  );
}

function UpNextCard({ item }: { item: UpNextItem }) {
  const { title_language } = usePreferences();
  const log = useLogUpNext();
  const name = displayTitle(item.title, title_language);
  const label = episodeLabel(item);
  return (
    <article
      aria-label={name}
      className="flex gap-3 rounded-xl border border-line bg-surface p-3 shadow-raised"
    >
      <Link
        to="/titles/$ref"
        params={{ ref: item.title.ref }}
        className="shrink-0 rounded-poster focus-visible:outline-2 focus-visible:outline-ring"
      >
        <Poster
          src={item.title.poster_url}
          title={name}
          size="xs"
          className="w-16"
        />
      </Link>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <Link
          to="/titles/$ref"
          params={{ ref: item.title.ref }}
          className="truncate rounded-sm text-sm font-semibold text-fg hover:underline focus-visible:outline-2 focus-visible:outline-ring"
        >
          {name}
        </Link>
        <p className="truncate text-sm text-fg">
          <span className="font-medium tabular-nums">{label}</span>
          {item.name && <span className="text-fg-muted"> · {item.name}</span>}
        </p>
        <p className="text-xs text-fg-muted">
          {item.aired
            ? item.last_watched_at
              ? `Last watched ${formatInstant(item.last_watched_at)}`
              : "Ready to watch"
            : item.air_date
              ? `Airs ${formatDay(item.air_date)}`
              : "Air date not announced"}
        </p>
        <div className="mt-auto flex justify-end">
          <Button
            size="sm"
            variant={item.aired ? "primary" : "secondary"}
            loading={log.isPending}
            onClick={() => log.mutate(item)}
            aria-label={`Log ${label} of ${name}`}
          >
            <Play aria-hidden className="size-4" />
            Log {label}
          </Button>
        </div>
      </div>
    </article>
  );
}

/** Movies on Planning: the other half of "what to watch". */
function PlannedMovies() {
  const { title_language } = usePreferences();
  const planned = useListLibrary({
    status: "planning",
    category: "movie",
    limit: 20,
  });
  const quick = useQuickLog();
  if (planned.isPending) return <Skeleton className="h-48" />;
  if (planned.isError || planned.data.items.length === 0) return null;
  return (
    <section aria-labelledby="movies-heading" className="flex flex-col gap-3">
      <h2
        id="movies-heading"
        className="flex items-center gap-2 font-display text-lg font-semibold text-fg"
      >
        <Clapperboard aria-hidden className="size-5 text-accent" />
        Movies to watch
      </h2>
      <ul className="-mx-1 flex gap-3 overflow-x-auto px-1 pb-2">
        {planned.data.items.map((entry: LibraryEntry) => {
          const name = displayTitle(entry.title, title_language);
          return (
            <li
              key={entry.title.ref}
              className="flex w-28 shrink-0 flex-col gap-1.5"
            >
              <Link
                to="/titles/$ref"
                params={{ ref: entry.title.ref }}
                className="flex flex-col gap-1.5 rounded-md focus-visible:outline-2 focus-visible:outline-ring"
              >
                <Poster
                  src={entry.title.poster_url}
                  title={name}
                  size="sm"
                  className="w-28"
                />
                <span className="line-clamp-2 text-xs font-medium text-fg">
                  {name}
                </span>
              </Link>
              <Button
                size="sm"
                variant="secondary"
                disabled={quick.isPending}
                onClick={() => quick.mutate(entry.title)}
                aria-label={`Mark ${name} watched`}
              >
                Watched
              </Button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
