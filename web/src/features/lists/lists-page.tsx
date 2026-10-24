import {
  infiniteQueryOptions,
  useInfiniteQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { ListVideo, Plus, Sparkles } from "lucide-react";
import { useState } from "react";
import {
  getListListsQueryKey,
  listLists,
  useCreateList,
} from "@/api/lists/lists";
import type { PageList } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { formatInstant } from "@/features/calendar/dates";
import { LoadMore } from "@/features/library/load-more";
import { reportFailure } from "@/features/titles/use-tracking";
import { ListFormDialog } from "./list-form";
import { invalidateLists } from "./use-lists";

function listsPagesOptions() {
  return infiniteQueryOptions({
    queryKey: [...getListListsQueryKey(), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listLists({ cursor: pageParam, limit: 50 }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageList) => last.next_cursor ?? undefined,
  });
}

/** Every list of the caller's, and a way to start a new one. */
export function ListsPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const pages = useInfiniteQuery(listsPagesOptions());
  const [creating, setCreating] = useState(false);
  const create = useCreateList({
    mutation: {
      onSuccess: (list) => {
        setCreating(false);
        toast({ tone: "success", title: `Created ${list.name}` });
        void navigate({ to: "/lists/$listId", params: { listId: list.id } });
      },
      onError: reportFailure,
      onSettled: () => invalidateLists(queryClient),
    },
  });

  let body: React.ReactNode;
  if (pages.isPending) {
    body = (
      <div aria-hidden className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {Array.from({ length: 6 }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
          <Skeleton key={i} className="h-24" />
        ))}
      </div>
    );
  } else if (pages.isError) {
    body = (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  } else {
    const lists = pages.data.pages.flatMap((page) => page.items);
    body =
      lists.length === 0 ? (
        <EmptyState
          icon={<ListVideo />}
          title="No lists yet"
          description="Collect titles your way: a watch party queue, the best of a year, comfort rewatches."
          action={
            <Button onClick={() => setCreating(true)}>
              <Plus aria-hidden className="size-4" />
              New list
            </Button>
          }
        />
      ) : (
        <>
          <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {lists.map((list) => (
              <li key={list.id}>
                <Link
                  to="/lists/$listId"
                  params={{ listId: list.id }}
                  className="flex h-full flex-col gap-1 rounded-xl border border-line bg-surface p-4 shadow-raised transition-colors hover:bg-surface-raised focus-visible:outline-2 focus-visible:outline-ring"
                >
                  <span className="flex items-center gap-2 font-semibold text-fg">
                    {list.smart && (
                      <Sparkles
                        aria-label="Smart list"
                        className="size-4 text-accent"
                      />
                    )}
                    <span className="truncate">{list.name}</span>
                  </span>
                  {list.description && (
                    <span className="line-clamp-2 text-sm text-fg-muted">
                      {list.description}
                    </span>
                  )}
                  <span className="mt-auto pt-2 text-xs text-fg-subtle">
                    {list.item_count != null
                      ? `${list.item_count} title${list.item_count === 1 ? "" : "s"}`
                      : "Filled by its filter"}
                    {" · updated "}
                    {formatInstant(list.updated_at)}
                  </span>
                </Link>
              </li>
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

  return (
    <>
      <PageHeader
        title="Lists"
        description="Your own collections, ordered the way you like."
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus aria-hidden className="size-4" />
            New list
          </Button>
        }
      />
      {body}
      <ListFormDialog
        open={creating}
        onOpenChange={setCreating}
        title="New list"
        submitLabel="Create list"
        pending={create.isPending}
        onSubmit={(fields) => create.mutate({ data: fields })}
      />
    </>
  );
}
