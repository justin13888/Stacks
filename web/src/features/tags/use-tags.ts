// Tag data shared by the Library, the title page and the tag manager.

import {
  type QueryClient,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { getListLibraryQueryKey } from "@/api/library/library";
import type { Tag, TitleRef } from "@/api/schemas";
import {
  getListTagsQueryKey,
  tagEntry,
  untagEntry,
  useListTags,
} from "@/api/tags/tags";
import { toast } from "@/components/ui/toast";
import { reportFailure } from "@/features/titles/use-tracking";

/** How many tags the pickers and filters read: every tag, in practice. */
export const ALL_TAGS = { limit: 200 } as const;

export function useAllTags() {
  return useListTags(ALL_TAGS);
}

/** Refetches the tags (their counts) and every Library listing. */
export async function invalidateTags(queryClient: QueryClient): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: getListTagsQueryKey() }),
    queryClient.invalidateQueries({ queryKey: getListLibraryQueryKey() }),
  ]);
}

export interface TagAssignment {
  tag: Tag;
  refs: readonly TitleRef[];
  /** `true` puts the tag on every title, `false` takes it off. */
  assign: boolean;
}

function titlesWord(count: number): string {
  return count === 1 ? "1 title" : `${count} titles`;
}

/** Puts a tag on (or takes it off) one title or many at once. */
export function useAssignTag() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ tag, refs, assign }: TagAssignment) => {
      const call = assign ? tagEntry : untagEntry;
      await Promise.all(refs.map((ref) => call(ref, tag.id)));
    },
    onSuccess: (_void, { tag, refs, assign }) => {
      toast({
        tone: "success",
        title: assign ? `Tagged ${tag.name}` : `Removed ${tag.name}`,
        description: titlesWord(refs.length),
      });
    },
    onError: reportFailure,
    onSettled: () => invalidateTags(queryClient),
  });
}
