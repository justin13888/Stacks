import { ListPlus, Tag as TagIcon } from "lucide-react";
import { useState } from "react";
import type { LibraryEntry, TitleDetail } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { AddToListDialog } from "@/features/lists/add-to-list";
import { ManageTagsDialog } from "@/features/tags/manage-tags";
import { TagAssignDialog } from "@/features/tags/tag-assign";
import { TagList } from "@/features/tags/tag-chip";

/**
 * The title page's organising actions: add the title to a list, and (once
 * it is in the library, which tags belong to) see and change its tags.
 */
export function TitleActions({
  title,
  name,
  entry,
}: {
  title: TitleDetail;
  /** The title in the user's title language. */
  name: string;
  /** The caller's entry for the title; `null` when it is not in the library. */
  entry: LibraryEntry | null;
}) {
  const [open, setOpen] = useState<"list" | "tags" | "manage" | null>(null);
  const onOpenChange = (which: typeof open) => (next: boolean) =>
    setOpen(next ? which : null);
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button variant="secondary" size="sm" onClick={() => setOpen("list")}>
        <ListPlus aria-hidden className="size-4" />
        Add to list
      </Button>
      {entry && (
        <>
          <Button variant="secondary" size="sm" onClick={() => setOpen("tags")}>
            <TagIcon aria-hidden className="size-4" />
            Tags
          </Button>
          <TagList tags={entry.tags} label={`Tags on ${name}`} />
          <TagAssignDialog
            titles={[{ ref: title.ref, tags: entry.tags }]}
            subject={name}
            open={open === "tags"}
            onOpenChange={onOpenChange("tags")}
            onManage={() => setOpen("manage")}
          />
        </>
      )}
      <AddToListDialog
        title={title}
        open={open === "list"}
        onOpenChange={onOpenChange("list")}
      />
      <ManageTagsDialog
        open={open === "manage"}
        onOpenChange={onOpenChange("manage")}
      />
    </div>
  );
}
