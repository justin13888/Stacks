import { ListPlus, Tag as TagIcon } from "lucide-react";
import { useState } from "react";
import type { TitleDetail } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { AddToListDialog } from "@/features/lists/add-to-list";
import { ManageTagsDialog } from "@/features/tags/manage-tags";
import { TagAssignDialog } from "@/features/tags/tag-assign";

/**
 * The title page's organising actions: add the title to a list, and (once
 * it is in the library, which tags belong to) tag it.
 */
export function TitleActions({
  title,
  name,
  inLibrary,
}: {
  title: TitleDetail;
  /** The title in the user's title language. */
  name: string;
  inLibrary: boolean;
}) {
  const [open, setOpen] = useState<"list" | "tags" | "manage" | null>(null);
  const onOpenChange = (which: typeof open) => (next: boolean) =>
    setOpen(next ? which : null);
  return (
    <div className="flex flex-wrap gap-2">
      <Button variant="secondary" size="sm" onClick={() => setOpen("list")}>
        <ListPlus aria-hidden className="size-4" />
        Add to list
      </Button>
      {inLibrary && (
        <Button variant="secondary" size="sm" onClick={() => setOpen("tags")}>
          <TagIcon aria-hidden className="size-4" />
          Tags
        </Button>
      )}
      <AddToListDialog
        title={title}
        open={open === "list"}
        onOpenChange={onOpenChange("list")}
      />
      <TagAssignDialog
        refs={[title.ref]}
        subject={name}
        open={open === "tags"}
        onOpenChange={onOpenChange("tags")}
        onManage={() => setOpen("manage")}
      />
      <ManageTagsDialog
        open={open === "manage"}
        onOpenChange={onOpenChange("manage")}
      />
    </div>
  );
}
