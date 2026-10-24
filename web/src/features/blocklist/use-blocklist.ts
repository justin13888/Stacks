// The blocklist ("Not interested"): blocking and unblocking a title, and the
// set of titles every catalog listing hides.
//
// The server already leaves blocked titles out of feeds, search and
// recommendations. A listing fetched before a block still holds the title,
// though, so the client hides what it knows is blocked: the first page of
// the blocklist, plus every title blocked in this session (which takes effect
// the moment the user asks, before the server answers).

import {
  type QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useMemo } from "react";
import {
  blockTitle,
  getListBlockedQueryKey,
  unblockTitle,
  useListBlocked,
} from "@/api/blocklist/blocklist";
import {
  getGetFeedQueryKey,
  getSearchTitlesQueryKey,
} from "@/api/catalog/catalog";
import { Feed, type TitleRef, type TitleSummary } from "@/api/schemas";
import { toast } from "@/components/ui/toast";
import { reportFailure } from "@/features/titles/use-tracking";

/** How many blocked titles the hidden set reads from the server. */
const BLOCKED_PAGE = 200;

/** The titles blocked (or unblocked) in this session, before the server says so. */
const SESSION_KEY = ["blocklist", "session"] as const;

interface SessionBlocks {
  blocked: ReadonlySet<TitleRef>;
  unblocked: ReadonlySet<TitleRef>;
}

const NO_BLOCKS: SessionBlocks = { blocked: new Set(), unblocked: new Set() };

function sessionBlocks(queryClient: QueryClient): SessionBlocks {
  return queryClient.getQueryData<SessionBlocks>(SESSION_KEY) ?? NO_BLOCKS;
}

function setBlocked(
  queryClient: QueryClient,
  ref: TitleRef,
  blocked: boolean,
): SessionBlocks {
  const previous = sessionBlocks(queryClient);
  const add = new Set(previous.blocked);
  const remove = new Set(previous.unblocked);
  if (blocked) {
    add.add(ref);
    remove.delete(ref);
  } else {
    add.delete(ref);
    remove.add(ref);
  }
  queryClient.setQueryData<SessionBlocks>(SESSION_KEY, {
    blocked: add,
    unblocked: remove,
  });
  return previous;
}

const FEED_PATHS: ReadonlySet<unknown> = new Set(
  Object.values(Feed).map((feed) => getGetFeedQueryKey(feed)[0]),
);

/**
 * Whether a query holds a catalog listing: a feed, a search or a title's
 * recommendations. Each shows titles with their library state, and each
 * leaves blocked titles out.
 */
export function isCatalogListing(queryKey: readonly unknown[]): boolean {
  const [path] = queryKey;
  return (
    FEED_PATHS.has(path) ||
    path === getSearchTitlesQueryKey()[0] ||
    (typeof path === "string" && path.endsWith("/recommendations"))
  );
}

/** The refs every catalog listing hides. */
export function useHiddenRefs(): ReadonlySet<TitleRef> {
  const blocked = useListBlocked({ limit: BLOCKED_PAGE });
  const session = useQuery({
    queryKey: SESSION_KEY,
    queryFn: () => NO_BLOCKS,
    initialData: NO_BLOCKS,
    staleTime: Number.POSITIVE_INFINITY,
  });
  const items = blocked.data?.items;
  return useMemo(() => {
    const hidden = new Set<TitleRef>(session.data.blocked);
    for (const item of items ?? []) {
      if (!session.data.unblocked.has(item.title.ref)) {
        hidden.add(item.title.ref);
      }
    }
    return hidden;
  }, [items, session.data]);
}

/** `titles` without the hidden ones. */
export function withoutHidden<T extends Pick<TitleSummary, "ref">>(
  titles: readonly T[],
  hidden: ReadonlySet<TitleRef>,
): T[] {
  return titles.filter((title) => !hidden.has(title.ref));
}

/** Unblocks a title: it shows in the catalog again. */
export function useUnblock() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (title: TitleSummary) => unblockTitle(title.ref),
    onMutate: (title) => ({
      previous: setBlocked(queryClient, title.ref, false),
    }),
    onSuccess: (_void, title) => {
      toast({ tone: "success", title: "Unblocked", description: title.title });
    },
    onError: (error, _title, context) => {
      if (context) queryClient.setQueryData(SESSION_KEY, context.previous);
      reportFailure(error);
    },
    onSettled: () => settle(queryClient),
  });
}

function settle(queryClient: QueryClient): Promise<void> {
  return queryClient.invalidateQueries({
    predicate: (query) =>
      query.queryKey[0] === getListBlockedQueryKey()[0] ||
      isCatalogListing(query.queryKey),
  });
}

/**
 * "Not interested": hides the title everywhere at once and blocks it; the
 * toast's undo unblocks it again.
 */
export function useNotInterested() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (title: TitleSummary) => blockTitle(title.ref),
    onMutate: (title) => ({
      previous: setBlocked(queryClient, title.ref, true),
    }),
    onSuccess: (_void, title) => {
      toast({
        title: "Hidden from Discover",
        description: title.title,
        action: {
          label: "Undo",
          onClick: () => {
            setBlocked(queryClient, title.ref, false);
            void unblockTitle(title.ref)
              .catch((error: unknown) => {
                setBlocked(queryClient, title.ref, true);
                reportFailure(error);
              })
              .finally(() => settle(queryClient));
          },
        },
      });
    },
    onError: (error, _title, context) => {
      if (context) queryClient.setQueryData(SESSION_KEY, context.previous);
      reportFailure(error);
    },
    onSettled: () => settle(queryClient),
  });
}
