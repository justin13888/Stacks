// A list's data: its items as an infinite query, where a reorder lands, and
// the reorder itself (optimistic, rolled back when the server refuses).

import {
  type InfiniteData,
  infiniteQueryOptions,
  type QueryClient,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import {
  getGetListQueryKey,
  getListListItemsQueryKey,
  getListListsQueryKey,
  listListItems,
  updateListItem,
} from "@/api/lists/lists";
import type {
  ItemPlacement,
  ListItem,
  ListItemPage,
  TitleRef,
} from "@/api/schemas";
import { reportFailure } from "@/features/titles/use-tracking";

/** Items per page of a list. */
export const ITEMS_PAGE_SIZE = 100;

/** A list's items, every page; the key extends the generated items key. */
export function listItemsPagesOptions(id: string) {
  return infiniteQueryOptions({
    queryKey: [...getListListItemsQueryKey(id), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listListItems(
        id,
        { cursor: pageParam, limit: ITEMS_PAGE_SIZE },
        { signal },
      ),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: ListItemPage) => last.next_cursor ?? undefined,
  });
}

type Pages = InfiniteData<ListItemPage, string | undefined>;

/** Refetches the lists overview, and one list with its items. */
export async function invalidateLists(
  queryClient: QueryClient,
  id?: string,
): Promise<void> {
  const keys: readonly (readonly unknown[])[] = [
    getListListsQueryKey(),
    ...(id ? [getGetListQueryKey(id), getListListItemsQueryKey(id)] : []),
  ];
  await Promise.all(
    keys.map((queryKey) => queryClient.invalidateQueries({ queryKey })),
  );
}

/** `items` with the one at `from` moved to `to`. */
export function moveItem<T>(
  items: readonly T[],
  from: number,
  to: number,
): T[] {
  const next = [...items];
  const [moved] = next.splice(from, 1);
  if (moved !== undefined) next.splice(to, 0, moved);
  return next;
}

/**
 * Where the item moved from `from` to `to` lands, as the API's placement:
 * directly after its new upper neighbour (`null`: the start) and before its
 * new lower one (`null`: the end). When it lands last of the items loaded so
 * far and more pages remain, its lower neighbour is not known, so only
 * `after` is sent.
 */
export function placementFor(
  items: readonly ListItem[],
  from: number,
  to: number,
  hasMore: boolean,
): ItemPlacement {
  const next = moveItem(items, from, to);
  const after = next[to - 1]?.title.ref ?? null;
  const below = next[to + 1]?.title.ref;
  if (below !== undefined) return { after, before: below };
  return hasMore ? { after } : { after, before: null };
}

function reordered(pages: Pages, from: number, to: number): Pages {
  // One flat order across the loaded pages, cut back into the same sizes.
  const sizes = pages.pages.map((page) => page.items.length);
  const flat = moveItem(
    pages.pages.flatMap((page) => page.items),
    from,
    to,
  );
  let offset = 0;
  return {
    ...pages,
    pages: pages.pages.map((page, index) => {
      const size = sizes[index] ?? 0;
      const items = flat.slice(offset, offset + size);
      offset += size;
      return { ...page, items };
    }),
  };
}

export interface ReorderRequest {
  ref: TitleRef;
  from: number;
  to: number;
  placement: ItemPlacement;
}

/** Moves a list item: at once on screen, back again if the server refuses. */
export function useReorderItem(id: string) {
  const queryClient = useQueryClient();
  const queryKey = listItemsPagesOptions(id).queryKey;
  return useMutation({
    mutationFn: ({ ref, placement }: ReorderRequest) =>
      updateListItem(id, ref, placement),
    onMutate: async ({ from, to }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<Pages>(queryKey);
      if (previous) {
        queryClient.setQueryData(queryKey, reordered(previous, from, to));
      }
      return { previous };
    },
    onError: (error, _request, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKey, context.previous);
      }
      reportFailure(error);
    },
    onSettled: () =>
      queryClient.invalidateQueries({
        queryKey: getListListItemsQueryKey(id),
      }),
  });
}
