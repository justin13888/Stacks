import { type FormEvent, useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

export interface ListFields {
  name: string;
  /** `null` when left empty. */
  description: string | null;
}

/** A list's name and description, for creating a list or editing one. */
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

  function submit(event: FormEvent) {
    event.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Give the list a name.");
      return;
    }
    onSubmit({ name: trimmed, description: description.trim() || null });
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
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
      <div className="flex justify-end">
        <Button type="submit" loading={pending}>
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}
