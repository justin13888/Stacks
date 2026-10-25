// Tag data shared by the Library, the title page and the tag manager.

import {
  type QueryClient,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import {
  getGetLibraryEntryQueryKey,
  getListLibraryQueryKey,
} from "@/api/library/library";
import type { Tag, TitleRef } from "@/api/schemas";
import {
  getListTagsQueryKey,
  tagEntry,
  untagEntry,
  useListTags,
} from "@/api/tags/tags";
import { describeError, reportFailure } from "@/components/ui/error-state";
import { toast } from "@/components/ui/toast";
import { isUnauthenticated } from "@/hooks/use-auth";
import { isApiProblem } from "@/lib/api-client";

/** How many tags the pickers and filters read: every tag, in practice. */
export const ALL_TAGS = { limit: 200 } as const;

export function useAllTags() {
  return useListTags(ALL_TAGS);
}

/** The start of every entry's query key: `getLibraryEntry`'s path. */
const ENTRY_KEY_PREFIX = getGetLibraryEntryQueryKey("" as TitleRef)[0];

/** Whether `queryKey` is one entry's (`getLibraryEntry`), not a listing's. */
function isEntryKey(queryKey: readonly unknown[]): boolean {
  const [path] = queryKey;
  return (
    typeof path === "string" &&
    path.startsWith(ENTRY_KEY_PREFIX) &&
    !path.slice(ENTRY_KEY_PREFIX.length).includes("/")
  );
}

/**
 * Refetches the tags (their counts), every Library listing, and every
 * entry read on its own, all of which name the tags they carry.
 */
export async function invalidateTags(queryClient: QueryClient): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: getListTagsQueryKey() }),
    queryClient.invalidateQueries({ queryKey: getListLibraryQueryKey() }),
    queryClient.invalidateQueries({
      predicate: (query) => isEntryKey(query.queryKey),
    }),
  ]);
}

export interface TagAssignment {
  tag: Tag;
  refs: readonly TitleRef[];
  /** `true` puts the tag on every title, `false` takes it off. */
  assign: boolean;
}

/** "1 title", "3 titles". */
export function titlesWord(count: number): string {
  return count === 1 ? "1 title" : `${count} titles`;
}

/**
 * How many tag writes a bulk change keeps in flight. A selection is sent a
 * few titles at a time rather than all at once, so a large selection stays
 * well inside the API's rate limit.
 */
export const BULK_CONCURRENCY = 4;

/** What a bulk change did: how many titles it changed, and the first failure. */
export interface BulkOutcome {
  done: number;
  total: number;
  /** The first failure, if any title was left unchanged. */
  error?: unknown;
}

/**
 * A failure that the remaining titles would hit too: the session ended, the
 * rate limit was reached, or the server is unavailable. Sending the rest
 * would only fail again, so the change stops there.
 */
function stopsTheRest(error: unknown): boolean {
  return (
    isApiProblem(error) &&
    (error.status === 401 || error.status === 403 || error.retryable)
  );
}

/**
 * Applies `call` to every ref, at most `concurrency` at a time. A failure on
 * one title leaves the others to go ahead, unless it is one the rest would
 * hit too; then nothing more is sent.
 */
export async function forEachBounded(
  refs: readonly TitleRef[],
  call: (ref: TitleRef) => Promise<unknown>,
  concurrency = BULK_CONCURRENCY,
): Promise<BulkOutcome> {
  let next = 0;
  let done = 0;
  let error: unknown;
  let stopped = false;
  async function worker(): Promise<void> {
    while (!stopped && next < refs.length) {
      const ref = refs[next++] as TitleRef;
      try {
        await call(ref);
        done += 1;
      } catch (failure) {
        error ??= failure;
        if (stopsTheRest(failure)) stopped = true;
      }
    }
  }
  const workers = Math.max(1, Math.min(concurrency, refs.length));
  await Promise.all(Array.from({ length: workers }, worker));
  return { done, total: refs.length, error };
}

/**
 * Puts a tag on (or takes it off) one title or many. A selection is sent a
 * few titles at a time, and a partial result says how many titles changed.
 */
export function useAssignTag() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ tag, refs, assign }: TagAssignment) => {
      const call = assign ? tagEntry : untagEntry;
      return forEachBounded(refs, (ref) => call(ref, tag.id));
    },
    onSuccess: ({ done, total, error }, { tag, assign }) => {
      if (error === undefined) {
        toast({
          tone: "success",
          title: assign ? `Tagged ${tag.name}` : `Removed ${tag.name}`,
          description: titlesWord(total),
        });
        return;
      }
      if (isUnauthenticated(error)) return;
      if (total === 1) {
        reportFailure(error);
        return;
      }
      const { detail } = describeError(error);
      const verb = assign
        ? `Tagged ${tag.name} on`
        : `Removed ${tag.name} from`;
      toast({
        tone: "error",
        title: `${verb} ${done} of ${titlesWord(total)}`,
        description: `${titlesWord(total - done)} unchanged: ${detail}`,
      });
    },
    onError: reportFailure,
    onSettled: () => invalidateTags(queryClient),
  });
}
