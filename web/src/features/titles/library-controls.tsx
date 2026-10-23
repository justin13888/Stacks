import { Heart } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import type { LibraryEntryDetail, Status, TitleRef } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { inputClass } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import { cn } from "@/lib/cn";
import type { ScoreFormat } from "@/lib/score-format";
import { ScoreInput } from "./score-input";
import { useAddEntry, useUpdateEntry } from "./use-tracking";

export const STATUSES: readonly Status[] = [
  "planning",
  "watching",
  "completed",
  "paused",
  "dropped",
];

const STATUS_OPTIONS = STATUSES.map((value) => ({
  value,
  label: STATUS_LABELS[value],
}));

/** How long typing pauses before notes save themselves. */
export const NOTES_AUTOSAVE_MS = 800;

/**
 * The entry's own settings: status, score, favourite and notes. A title not
 * in the library offers only "add with a status".
 */
export function LibraryControls({
  titleRef,
  entry,
  scoreFormat,
}: {
  titleRef: TitleRef;
  entry: LibraryEntryDetail | null;
  scoreFormat: ScoreFormat;
}) {
  const update = useUpdateEntry(titleRef);
  const add = useAddEntry(titleRef);

  if (!entry) {
    return (
      <section
        aria-label="Your library"
        className="flex flex-wrap items-end gap-3"
      >
        <Select
          label="Add to library"
          value={undefined}
          placeholder="Choose a status…"
          options={STATUS_OPTIONS}
          onValueChange={(status) => add.mutate(status)}
          disabled={add.isPending}
          className="w-56"
        />
      </section>
    );
  }

  return (
    <section aria-label="Your library" className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end gap-4">
        <Select
          label="Status"
          value={entry.status}
          options={STATUS_OPTIONS}
          onValueChange={(status) => {
            if (status !== entry.status) update.mutate({ status });
          }}
          className="w-44"
        />
        <ScoreInput
          score={entry.score}
          format={scoreFormat}
          onChange={(score) => update.mutate({ score })}
        />
        <Button
          variant="secondary"
          aria-pressed={entry.favourite}
          onClick={() => update.mutate({ favourite: !entry.favourite })}
        >
          <Heart
            aria-hidden
            className={cn(
              "size-4",
              entry.favourite && "fill-danger text-danger",
            )}
          />
          Favourite
        </Button>
      </div>
      <NotesEditor
        notes={entry.notes ?? ""}
        onSave={(notes) =>
          update.mutate({ notes: notes === "" ? null : notes })
        }
      />
    </section>
  );
}

/**
 * Notes that save themselves: after a pause in typing, and when the field
 * loses focus. Only a change is saved, and only `notes` is sent.
 */
export function NotesEditor({
  notes,
  onSave,
}: {
  notes: string;
  onSave: (notes: string) => void;
}) {
  const id = useId();
  const [text, setText] = useState(notes);
  const saved = useRef(notes);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [status, setStatus] = useState<"idle" | "pending" | "saved">("idle");

  function flush(value: string) {
    clearTimeout(timer.current);
    timer.current = undefined;
    if (value === saved.current) {
      setStatus((s) => (s === "pending" ? "idle" : s));
      return;
    }
    saved.current = value;
    onSave(value);
    setStatus("saved");
  }

  // A pending save still happens when the page goes away.
  const latest = useRef({ text, flush });
  latest.current = { text, flush };
  useEffect(
    () => () => {
      if (latest.current && timer.current !== undefined) {
        latest.current.flush(latest.current.text);
      }
    },
    [],
  );

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between">
        <label htmlFor={id} className="text-sm font-medium text-fg">
          Notes
        </label>
        <span aria-live="polite" className="text-xs text-fg-subtle">
          {status === "pending"
            ? "Editing…"
            : status === "saved"
              ? "Saved"
              : ""}
        </span>
      </div>
      <textarea
        id={id}
        value={text}
        rows={3}
        maxLength={10000}
        placeholder="Thoughts, where you left off, who recommended it…"
        onChange={(event) => {
          const value = event.target.value;
          setText(value);
          setStatus("pending");
          clearTimeout(timer.current);
          timer.current = setTimeout(() => flush(value), NOTES_AUTOSAVE_MS);
        }}
        onBlur={() => flush(text)}
        className={cn(inputClass, "h-auto min-h-20 py-2")}
      />
    </div>
  );
}
