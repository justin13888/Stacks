import { useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { type FormEvent, useState } from "react";
import type { Tag } from "@/api/schemas";
import { useCreateTag, useDeleteTag, useUpdateTag } from "@/api/tags/tags";
import { Button, IconButton } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { ErrorState, reportFailure } from "@/components/ui/error-state";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { invalidateTags, titlesWord, useAllTags } from "./use-tags";

/** The colour a new tag starts with: the accent hue. */
const DEFAULT_COLOR = "#7c5cff";

const colorClass =
  "h-10 w-12 shrink-0 cursor-pointer rounded-md border border-line bg-surface p-1 focus-visible:outline-2 focus-visible:outline-ring";

/** Create, rename, recolour and delete tags. */
export function ManageTagsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Manage tags"
        description="Tags label titles in your library; the Library filters by them."
        className="w-[min(92vw,36rem)]"
      >
        {open && <TagManager />}
      </DialogContent>
    </Dialog>
  );
}

function TagManager() {
  const tags = useAllTags();
  return (
    <div className="flex min-h-0 flex-col gap-4">
      <NewTagForm />
      {tags.isPending ? (
        <Skeleton className="h-24" />
      ) : tags.isError ? (
        <ErrorState error={tags.error} onRetry={() => void tags.refetch()} />
      ) : tags.data.items.length === 0 ? (
        <p className="text-sm text-fg-muted">No tags yet.</p>
      ) : (
        <ul className="flex max-h-80 flex-col gap-2 overflow-y-auto">
          {tags.data.items.map((tag) => (
            <TagRow key={tag.id} tag={tag} />
          ))}
        </ul>
      )}
    </div>
  );
}

function NewTagForm() {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [color, setColor] = useState(DEFAULT_COLOR);
  const create = useCreateTag({
    mutation: {
      onSuccess: (tag) => {
        setName("");
        toast({ tone: "success", title: `Created ${tag.name}` });
      },
      onError: reportFailure,
      onSettled: () => invalidateTags(queryClient),
    },
  });
  function submit(event: FormEvent) {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed) create.mutate({ data: { name: trimmed, color } });
  }
  return (
    <form onSubmit={submit} className="flex items-end gap-2">
      <div className="flex-1">
        <Input
          label="New tag"
          placeholder="e.g. Comfort watch"
          value={name}
          maxLength={100}
          onChange={(event) => setName(event.target.value)}
        />
      </div>
      <input
        type="color"
        aria-label="New tag colour"
        value={color}
        onChange={(event) => setColor(event.target.value)}
        className={colorClass}
      />
      <Button type="submit" loading={create.isPending} disabled={!name.trim()}>
        <Plus aria-hidden className="size-4" />
        Create
      </Button>
    </form>
  );
}

function TagRow({ tag }: { tag: Tag }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(tag.name);
  const [color, setColor] = useState(tag.color ?? DEFAULT_COLOR);
  const update = useUpdateTag({
    mutation: {
      onError: (error) => {
        setName(tag.name);
        setColor(tag.color ?? DEFAULT_COLOR);
        reportFailure(error);
      },
      onSettled: () => invalidateTags(queryClient),
    },
  });
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteTag({
    mutation: {
      onSuccess: () => toast({ title: `Deleted ${tag.name}` }),
      onError: (error) => {
        setConfirming(false);
        reportFailure(error);
      },
      onSettled: () => invalidateTags(queryClient),
    },
  });

  if (confirming) {
    return (
      <ConfirmDelete
        tag={tag}
        // Stays busy after success, until the refetch removes the row.
        pending={remove.isPending || remove.isSuccess}
        onConfirm={() => remove.mutate({ id: tag.id })}
        onCancel={() => setConfirming(false)}
      />
    );
  }

  function rename() {
    const trimmed = name.trim();
    if (!trimmed) {
      setName(tag.name);
      return;
    }
    if (trimmed !== tag.name) {
      update.mutate({ id: tag.id, data: { name: trimmed } });
    }
  }

  return (
    <li className="flex items-center gap-2">
      <input
        type="color"
        aria-label={`Colour of ${tag.name}`}
        value={color}
        // React's onChange follows every step of the picker; the colour is
        // saved once the picker lets go.
        onChange={(event) => setColor(event.target.value)}
        onBlur={() => {
          if (color !== (tag.color ?? DEFAULT_COLOR)) {
            update.mutate({ id: tag.id, data: { color } });
          }
        }}
        className={colorClass}
      />
      <form
        className="flex-1"
        onSubmit={(event) => {
          event.preventDefault();
          rename();
        }}
      >
        <Input
          aria-label={`Name of ${tag.name}`}
          value={name}
          maxLength={100}
          onChange={(event) => setName(event.target.value)}
          onBlur={rename}
        />
      </form>
      <span className="w-16 text-right text-xs text-fg-subtle">
        {titlesWord(tag.entry_count)}
      </span>
      <IconButton
        label={`Delete ${tag.name}`}
        icon={<Trash2 aria-hidden className="size-4" />}
        onClick={() => setConfirming(true)}
      />
    </li>
  );
}

/**
 * Deleting a tag cannot be undone: the server removes it from every title it
 * is on. So the row asks first, and says how many titles will lose it.
 */
function ConfirmDelete({
  tag,
  pending,
  onConfirm,
  onCancel,
}: {
  tag: Tag;
  pending: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const warning =
    tag.entry_count === 0
      ? `Delete ${tag.name}? It is not on any title.`
      : `Delete ${tag.name}? It comes off ${titlesWord(tag.entry_count)}, and this cannot be undone.`;
  return (
    <li>
      <fieldset
        aria-label={`Delete ${tag.name}`}
        className="flex flex-wrap items-center gap-2 rounded-md border border-danger/40 px-3 py-2"
      >
        <p role="alert" className="min-w-0 flex-1 text-sm text-fg">
          {warning}
        </p>
        <Button variant="ghost" size="sm" onClick={onCancel} autoFocus>
          Cancel
        </Button>
        <Button
          variant="danger"
          size="sm"
          loading={pending}
          onClick={onConfirm}
        >
          Delete tag
        </Button>
      </fieldset>
    </li>
  );
}
