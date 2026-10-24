import { Minus, Plus } from "lucide-react";
import type { TitleRef } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { TagSwatch } from "./tag-chip";
import { useAllTags, useAssignTag } from "./use-tags";

/**
 * Puts tags on, or takes them off, the titles in `refs`: one title from its
 * page, or the Library's selection in bulk. Each tag offers both, since the
 * API does not say which tags a title already carries.
 */
export function TagAssignDialog({
  refs,
  subject,
  open,
  onOpenChange,
  onManage,
}: {
  refs: readonly TitleRef[];
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
        {open && <TagRows refs={refs} onManage={onManage} />}
      </DialogContent>
    </Dialog>
  );
}

function TagRows({
  refs,
  onManage,
}: {
  refs: readonly TitleRef[];
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
  return (
    <div className="flex flex-col gap-3">
      <ul className="flex max-h-80 flex-col gap-1 overflow-y-auto">
        {tags.data.items.map((tag) => {
          const busy = assign.isPending && assign.variables?.tag.id === tag.id;
          return (
            <li key={tag.id} className="flex items-center gap-2 px-1 py-1">
              <TagSwatch color={tag.color} />
              <span className="min-w-0 flex-1 truncate text-sm text-fg">
                {tag.name}
              </span>
              <Button
                variant="secondary"
                size="sm"
                aria-label={`Add ${tag.name}`}
                disabled={busy}
                onClick={() => assign.mutate({ tag, refs, assign: true })}
              >
                <Plus aria-hidden className="size-4" />
                Add
              </Button>
              <Button
                variant="ghost"
                size="sm"
                aria-label={`Remove ${tag.name}`}
                disabled={busy}
                onClick={() => assign.mutate({ tag, refs, assign: false })}
              >
                <Minus aria-hidden className="size-4" />
                Remove
              </Button>
            </li>
          );
        })}
      </ul>
      {manage}
    </div>
  );
}
