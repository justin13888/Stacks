import { ListPlus } from "lucide-react";
import { useState } from "react";
import type { TitleDetail } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { AddToListDialog } from "@/features/lists/add-to-list";

/** The title page's organising actions: add the title to a list. */
export function TitleActions({ title }: { title: TitleDetail }) {
  const [listOpen, setListOpen] = useState(false);
  return (
    <div className="flex flex-wrap gap-2">
      <Button variant="secondary" size="sm" onClick={() => setListOpen(true)}>
        <ListPlus aria-hidden className="size-4" />
        Add to list
      </Button>
      <AddToListDialog
        title={title}
        open={listOpen}
        onOpenChange={setListOpen}
      />
    </div>
  );
}
