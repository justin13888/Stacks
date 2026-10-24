import { Settings2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { ManageTagsDialog } from "./manage-tags";
import { TagChip } from "./tag-chip";
import { useAllTags } from "./use-tags";

/**
 * The Library's tag chips: pressing one filters to its titles (the API
 * filters by one tag at a time), pressing it again clears the filter.
 */
export function TagFilter({
  value,
  onChange,
}: {
  /** The selected tag id. */
  value: string | undefined;
  onChange: (tag: string | undefined) => void;
}) {
  const tags = useAllTags();
  const [managing, setManaging] = useState(false);
  const items = tags.data?.items ?? [];
  return (
    <div className="mb-6 flex flex-wrap items-center gap-2">
      {items.length > 0 && (
        <fieldset className="flex flex-wrap items-center gap-2">
          <legend className="sr-only">Filter by tag</legend>
          {items.map((tag) => (
            <TagChip
              key={tag.id}
              tag={tag}
              pressed={value === tag.id}
              onClick={() => onChange(value === tag.id ? undefined : tag.id)}
            />
          ))}
        </fieldset>
      )}
      <Button variant="ghost" size="sm" onClick={() => setManaging(true)}>
        <Settings2 aria-hidden className="size-4" />
        {items.length > 0 ? "Manage tags" : "Create tags"}
      </Button>
      <ManageTagsDialog open={managing} onOpenChange={setManaging} />
    </div>
  );
}
