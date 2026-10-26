import { ListVideo, Sparkles } from "lucide-react";
import { RadioGroup } from "radix-ui";
import { type FormEvent, type ReactNode, useState } from "react";
import type { ListFilter } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/cn";
import { ListFilterEditor } from "./list-filter-editor";
import {
  draftFromFilter,
  type FilterErrors,
  filterFromDraft,
} from "./smart-filter";

export interface ListFields {
  name: string;
  /** `null` when left empty. */
  description: string | null;
  /** A smart list's filter; absent for a manual list. */
  filter?: ListFilter;
}

type Mode = "manual" | "smart";

function ModeOption({
  value,
  icon,
  title,
  description,
}: {
  value: Mode;
  icon: ReactNode;
  title: string;
  description: string;
}) {
  return (
    <RadioGroup.Item
      value={value}
      className={cn(
        "flex items-start gap-3 rounded-lg border border-line bg-surface p-3 text-left shadow-raised",
        "transition-colors duration-(--duration-fast) hover:bg-surface-raised",
        "focus-visible:outline-2 focus-visible:outline-ring",
        "data-[state=checked]:border-accent data-[state=checked]:bg-accent-soft",
      )}
    >
      <span aria-hidden className="mt-0.5 text-accent [&_svg]:size-5">
        {icon}
      </span>
      <span className="flex flex-col gap-0.5">
        <span className="text-sm font-medium text-fg">{title}</span>
        <span className="text-xs text-fg-muted">{description}</span>
      </span>
    </RadioGroup.Item>
  );
}

/**
 * A list's name and description, and a smart list's filter, for creating a
 * list or editing one. A new list can be either kind; an existing one keeps
 * its kind (`initial.filter` present for a smart list).
 */
export function ListFormDialog({
  open,
  onOpenChange,
  title,
  submitLabel,
  initial,
  pending,
  onSubmit,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  submitLabel: string;
  initial?: ListFields;
  pending: boolean;
  onSubmit: (fields: ListFields) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={title}>
        {open && (
          <ListForm
            initial={initial}
            submitLabel={submitLabel}
            pending={pending}
            onSubmit={onSubmit}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function ListForm({
  initial,
  submitLabel,
  pending,
  onSubmit,
}: {
  initial?: ListFields;
  submitLabel: string;
  pending: boolean;
  onSubmit: (fields: ListFields) => void;
}) {
  const [name, setName] = useState(initial?.name ?? "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [error, setError] = useState<string>();
  const [mode, setMode] = useState<Mode>(initial?.filter ? "smart" : "manual");
  const [draft, setDraft] = useState(() => draftFromFilter(initial?.filter));
  const [filterErrors, setFilterErrors] = useState<FilterErrors>({});

  function submit(event: FormEvent) {
    event.preventDefault();
    const trimmed = name.trim();
    const read = mode === "smart" ? filterFromDraft(draft) : undefined;
    setFilterErrors(read && "errors" in read ? read.errors : {});
    if (!trimmed) setError("Give the list a name.");
    if (!trimmed || (read && "errors" in read)) return;
    onSubmit({
      name: trimmed,
      description: description.trim() || null,
      ...(read && "filter" in read ? { filter: read.filter } : {}),
    });
  }

  return (
    <form
      onSubmit={submit}
      className="-mx-1 flex min-h-0 flex-col gap-4 overflow-y-auto px-1 pb-1"
      noValidate
    >
      {!initial && (
        <RadioGroup.Root
          aria-label="Kind of list"
          value={mode}
          onValueChange={(value) => setMode(value as Mode)}
          className="grid gap-2 sm:grid-cols-2"
        >
          <ModeOption
            value="manual"
            icon={<ListVideo />}
            title="Manual"
            description="You add the titles and put them in order."
          />
          <ModeOption
            value="smart"
            icon={<Sparkles />}
            title="Smart"
            description="A filter picks titles from a feed and keeps it fresh."
          />
        </RadioGroup.Root>
      )}
      <Input
        label="Name"
        value={name}
        maxLength={200}
        error={error}
        onChange={(event) => {
          setName(event.target.value);
          setError(undefined);
        }}
        autoFocus
      />
      <Input
        label="Description"
        hint="Optional."
        value={description}
        onChange={(event) => setDescription(event.target.value)}
      />
      {mode === "smart" && (
        <section
          aria-label="Filter"
          className="flex flex-col gap-4 border-t border-line pt-4"
        >
          <p className="text-xs text-fg-muted">
            Leave a criterion empty and it does not narrow the list.
          </p>
          <ListFilterEditor
            draft={draft}
            errors={filterErrors}
            onChange={(next) => {
              setDraft(next);
              setFilterErrors({});
            }}
          />
        </section>
      )}
      <div className="flex justify-end">
        <Button type="submit" loading={pending}>
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}
