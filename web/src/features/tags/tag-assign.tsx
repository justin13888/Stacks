import type { Tag, TitleRef } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { TagChip } from "./tag-chip";
import { titlesWord, useAllTags, useAssignTag } from "./use-tags";

/** A title the dialog tags, with the tags it carries now. */
export interface TaggedTitle {
  ref: TitleRef;
  tags: readonly Pick<Tag, "id">[];
}

/** Which of `titles` carry `tag`. */
export function carriers(
  titles: readonly TaggedTitle[],
  tag: Pick<Tag, "id">,
): TaggedTitle[] {
  return titles.filter((t) => t.tags.some((carried) => carried.id === tag.id));
}

/**
 * Puts tags on, or takes them off, `titles`: one title from its page, or the
 * Library's selection in bulk. Each tag is a toggle showing whether the
 * titles carry it: pressed when every one does, mixed when only some do.
 * Pressing a pressed tag takes it off them all; pressing any other puts it
 * on the titles still without it.
 */
export function TagAssignDialog({
  titles,
  subject,
  open,
  onOpenChange,
  onManage,
}: {
  titles: readonly TaggedTitle[];
  /** What the dialog tags, e.g. a title's name or "3 titles". */
  subject: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Opens the tag manager, for when the tag wanted does not exist yet. */
  onManage?: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="Tags" description={`Tag ${subject}.`}>
        {open && <TagToggles titles={titles} onManage={onManage} />}
      </DialogContent>
    </Dialog>
  );
}

function TagToggles({
  titles,
  onManage,
}: {
  titles: readonly TaggedTitle[];
  onManage?: () => void;
}) {
  const tags = useAllTags();
  const assign = useAssignTag();
  if (tags.isPending) return <Skeleton className="h-24" />;
  if (tags.isError) {
    return (
      <ErrorState error={tags.error} onRetry={() => void tags.refetch()} />
    );
  }
  const manage = onManage && (
    <Button variant="ghost" size="sm" onClick={onManage} className="self-start">
      Manage tags
    </Button>
  );
  if (tags.data.items.length === 0) {
    return (
      <div className="flex flex-col gap-2">
        <p className="text-sm text-fg-muted">You have no tags yet.</p>
        {manage}
      </div>
    );
  }
  const bulk = titles.length > 1;
  return (
    <div className="flex flex-col gap-3">
      <ul
        aria-label="Tags"
        className="flex max-h-80 flex-wrap gap-2 overflow-y-auto p-0.5"
      >
        {tags.data.items.map((tag) => {
          const on = carriers(titles, tag);
          const every = titles.length > 0 && on.length === titles.length;
          const pressed = every ? true : on.length > 0 ? "mixed" : false;
          const busy = assign.isPending && assign.variables?.tag.id === tag.id;
          const toggle = () => {
            const refs = (
              every ? on : titles.filter((t) => !on.includes(t))
            ).map((t) => t.ref);
            assign.mutate({ tag, refs, assign: !every });
          };
          return (
            <li key={tag.id}>
              <TagChip
                tag={tag}
                pressed={pressed}
                disabled={busy}
                aria-busy={busy || undefined}
                title={
                  bulk && pressed === "mixed"
                    ? `On ${on.length} of ${titlesWord(titles.length)}`
                    : undefined
                }
                onClick={toggle}
              />
            </li>
          );
        })}
      </ul>
      {bulk && (
        <p className="text-xs text-fg-subtle">
          A dashed tag is on only some of these titles; pressing it puts it on
          the rest.
        </p>
      )}
      {manage}
    </div>
  );
}
