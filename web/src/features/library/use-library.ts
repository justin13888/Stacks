// The Library's data: the keyset-paged listing as an infinite query, moving
// an entry to another status (the board), and the cards' quick "+1".

import {
  type InfiniteData,
  infiniteQueryOptions,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import {
  getListLibraryQueryKey,
  listLibrary,
  updateLibraryEntry,
} from "@/api/library/library";
import type {
  LibraryEntry,
  ListLibraryParams,
  PageLibraryEntry,
  Status,
  TitleSummary,
} from "@/api/schemas";
import { toast } from "@/components/ui/toast";
import { episodeLabel } from "@/features/titles/episodes";
import { reportFailure } from "@/features/titles/use-tracking";
import { invalidateTracking, logEpisode, resolveNextEpisode } from "./entry";

/** Entries per page of the listing. */
export const PAGE_SIZE = 50;

/**
 * One listing as an infinite query: each page's `next_cursor` fetches the
 * next, and a `null` one ends it. The key extends the generated listing key,
 * so invalidating `getListLibraryQueryKey()` refreshes every listing.
 */
export function libraryPagesOptions(params: ListLibraryParams) {
  return infiniteQueryOptions({
    queryKey: [...getListLibraryQueryKey(params), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listLibrary(
        { ...params, cursor: pageParam, limit: PAGE_SIZE },
        { signal },
      ),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageLibraryEntry) => last.next_cursor ?? undefined,
  });
}

type Pages = InfiniteData<PageLibraryEntry, string | undefined>;

function without(pages: Pages, ref: string): Pages {
  return {
    ...pages,
    pages: pages.pages.map((page) => ({
      ...page,
      items: page.items.filter((item) => item.title.ref !== ref),
    })),
  };
}

function prepend(pages: Pages, entry: LibraryEntry): Pages {
  const [first, ...rest] = pages.pages;
  if (!first) return pages;
  return {
    ...pages,
    pages: [{ ...first, items: [entry, ...first.items] }, ...rest],
  };
}

export interface MoveRequest {
  entry: LibraryEntry;
  to: Status;
  /** The listing the entry is in, and the one it moves to. */
  fromParams: ListLibraryParams;
  toParams: ListLibraryParams;
}

/**
 * Changes an entry's status from the board: the card moves columns at once
 * and moves back if the server refuses.
 */
export function useMoveEntry() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ entry, to }: MoveRequest) =>
      updateLibraryEntry(entry.title.ref, { status: to }),
    onMutate: async ({ entry, to, fromParams, toParams }) => {
      const fromKey = libraryPagesOptions(fromParams).queryKey;
      const toKey = libraryPagesOptions(toParams).queryKey;
      await queryClient.cancelQueries({ queryKey: getListLibraryQueryKey() });
      const before = {
        from: queryClient.getQueryData<Pages>(fromKey),
        to: queryClient.getQueryData<Pages>(toKey),
      };
      if (before.from) {
        queryClient.setQueryData(
          fromKey,
          without(before.from, entry.title.ref),
        );
      }
      if (before.to) {
        queryClient.setQueryData(
          toKey,
          prepend(before.to, { ...entry, status: to }),
        );
      }
      return { fromKey, toKey, before };
    },
    onSuccess: (_entry, { entry, to }) => {
      toast({
        tone: "success",
        title: `Moved to ${to.charAt(0).toUpperCase()}${to.slice(1)}`,
        description: entry.title.title,
      });
    },
    onError: (error, _request, context) => {
      if (context) {
        queryClient.setQueryData(context.fromKey, context.before.from);
        queryClient.setQueryData(context.toKey, context.before.to);
      }
      reportFailure(error);
    },
    onSettled: (_data, _error, { entry }) =>
      invalidateTracking(queryClient, entry.title.ref),
  });
}

/**
 * "+1" from a card: the next episode of a show (resolved the way the title
 * page does), or one more play of a movie.
 */
export function useQuickLog() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (title: TitleSummary) => {
      if (title.kind === "movie") {
        await logEpisode(title.ref, null);
        return "Marked as watched";
      }
      const next = await resolveNextEpisode(queryClient, title.ref);
      if (!next) return null;
      await logEpisode(title.ref, next);
      return `Logged ${episodeLabel(next)}`;
    },
    onSuccess: (message, title) => {
      toast(
        message
          ? { tone: "success", title: message, description: title.title }
          : { title: "You're all caught up", description: title.title },
      );
    },
    onError: reportFailure,
    onSettled: (_data, _error, title) =>
      invalidateTracking(queryClient, title.ref),
  });
}
