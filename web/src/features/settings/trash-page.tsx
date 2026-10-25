import {
  infiniteQueryOptions,
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { ListVideo, Trash2 } from "lucide-react";
import { restoreLibraryEntry } from "@/api/library/library";
import {
  getListListsQueryKey,
  listLists,
  restoreList,
} from "@/api/lists/lists";
import type { LibraryEntry, List, PageList } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { CATEGORY_NAMES } from "@/components/search/categories";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState, reportFailure } from "@/components/ui/error-state";
import { LoadMore } from "@/components/ui/load-more";
import { Poster } from "@/components/ui/poster";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { invalidateTracking } from "@/features/library/entry";
import { libraryPagesOptions } from "@/features/library/use-library";
import { invalidateLists } from "@/features/lists/use-lists";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { formatInstant } from "@/lib/format";
import { Meta, SettingsSection } from "./section";
import { SettingsNav } from "./settings-nav";

function trashedListsOptions() {
  return infiniteQueryOptions({
    queryKey: [...getListListsQueryKey({ deleted: true }), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listLists({ deleted: true, cursor: pageParam, limit: 50 }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageList) => last.next_cursor ?? undefined,
  });
}

/**
 * `/settings/trash`: library entries and lists that were deleted. Deleting
 * only moves them here, with their scores, plays, notes and items intact,
 * and restoring puts them back as they were.
 */
export function TrashPage() {
  return (
    <>
      <PageHeader
        title="Trash"
        description="What you deleted, kept with everything it held. Restore puts it back as it was."
      />
      <SettingsNav />
      <div className="flex flex-col gap-6">
        <TrashedEntries />
        <TrashedLists />
      </div>
    </>
  );
}

function Rows({ children }: { children: React.ReactNode }) {
  return <ul className="flex flex-col divide-y divide-line">{children}</ul>;
}

function Loading() {
  return (
    <div aria-hidden className="flex flex-col gap-2">
      <Skeleton className="h-14" />
      <Skeleton className="h-14" />
    </div>
  );
}

function TrashedEntries() {
  const { title_language } = usePreferences();
  const queryClient = useQueryClient();
  const pages = useInfiniteQuery(libraryPagesOptions({ deleted: true }));
  const restore = useMutation({
    mutationFn: (entry: LibraryEntry) => restoreLibraryEntry(entry.title.ref),
    onSuccess: (_restored, entry) =>
      toast({
        tone: "success",
        title: "Restored to your library",
        description: displayTitle(entry.title, title_language),
      }),
    onError: reportFailure,
    onSettled: (_r, _e, entry) =>
      invalidateTracking(queryClient, entry.title.ref),
  });

  let body: React.ReactNode;
  if (pages.isPending) body = <Loading />;
  else if (pages.isError)
    body = (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  else {
    const items = pages.data.pages.flatMap((page) => page.items);
    body =
      items.length === 0 ? (
        <EmptyState
          className="py-6"
          icon={<Trash2 />}
          title="No deleted titles"
          description="A title removed from your library waits here."
        />
      ) : (
        <>
          <Rows>
            {items.map((entry) => {
              const name = displayTitle(entry.title, title_language);
              return (
                <li
                  key={entry.title.ref}
                  className="flex flex-wrap items-center gap-3 py-3"
                >
                  <Poster src={entry.title.poster_url} title={name} size="xs" />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate font-medium text-fg">{name}</span>
                    <Meta>
                      {[entry.title.year, CATEGORY_NAMES[entry.title.category]]
                        .filter(Boolean)
                        .join(" · ")}
                      {entry.deleted_at
                        ? ` · deleted ${formatInstant(entry.deleted_at)}`
                        : ""}
                    </Meta>
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    aria-label={`Restore ${name}`}
                    loading={
                      restore.isPending &&
                      restore.variables?.title.ref === entry.title.ref
                    }
                    onClick={() => restore.mutate(entry)}
                  >
                    Restore
                  </Button>
                </li>
              );
            })}
          </Rows>
          <LoadMore
            hasNextPage={pages.hasNextPage}
            isFetchingNextPage={pages.isFetchingNextPage}
            fetchNextPage={() => pages.fetchNextPage()}
          />
        </>
      );
  }
  return (
    <SettingsSection title="Library" description="Deleted library entries.">
      {body}
    </SettingsSection>
  );
}

function TrashedLists() {
  const queryClient = useQueryClient();
  const pages = useInfiniteQuery(trashedListsOptions());
  const restore = useMutation({
    mutationFn: (list: List) => restoreList(list.id),
    onSuccess: (_restored, list) =>
      toast({
        tone: "success",
        title: "Restored to your lists",
        description: list.name,
      }),
    onError: reportFailure,
    onSettled: (_r, _e, list) => invalidateLists(queryClient, list.id),
  });

  let body: React.ReactNode;
  if (pages.isPending) body = <Loading />;
  else if (pages.isError)
    body = (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  else {
    const items = pages.data.pages.flatMap((page) => page.items);
    body =
      items.length === 0 ? (
        <EmptyState
          className="py-6"
          icon={<ListVideo />}
          title="No deleted lists"
          description="A deleted list waits here with its titles."
        />
      ) : (
        <>
          <Rows>
            {items.map((list) => (
              <li
                key={list.id}
                className="flex flex-wrap items-center gap-3 py-3"
              >
                <div className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate font-medium text-fg">
                    {list.name}
                  </span>
                  <Meta>
                    {list.smart
                      ? "Smart list"
                      : `${list.item_count ?? 0} ${list.item_count === 1 ? "title" : "titles"}`}
                    {list.deleted_at
                      ? ` · deleted ${formatInstant(list.deleted_at)}`
                      : ""}
                  </Meta>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  aria-label={`Restore ${list.name}`}
                  loading={
                    restore.isPending && restore.variables?.id === list.id
                  }
                  onClick={() => restore.mutate(list)}
                >
                  Restore
                </Button>
              </li>
            ))}
          </Rows>
          <LoadMore
            hasNextPage={pages.hasNextPage}
            isFetchingNextPage={pages.isFetchingNextPage}
            fetchNextPage={() => pages.fetchNextPage()}
          />
        </>
      );
  }
  return (
    <SettingsSection
      title="Lists"
      description="Deleted lists, with their titles."
    >
      {body}
    </SettingsSection>
  );
}
