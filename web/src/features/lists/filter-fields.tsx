// The fields a smart list's filter editor is built from: a labelled group,
// a pair of bounds, the genre chips, and a set of toggle chips.

import { Plus, X } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useId } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/cn";
import { withGenre } from "./smart-filter";

/** Genres the providers use, offered as the genre field's suggestions. */
const GENRE_SUGGESTIONS = [
  "Action",
  "Adventure",
  "Animation",
  "Comedy",
  "Crime",
  "Documentary",
  "Drama",
  "Family",
  "Fantasy",
  "History",
  "Horror",
  "Music",
  "Mystery",
  "Romance",
  "Science Fiction",
  "Sci-Fi & Fantasy",
  "Thriller",
  "War",
  "Western",
];

export const chipClass =
  "inline-flex h-8 items-center gap-1.5 rounded-full border border-line bg-surface px-3 text-sm text-fg-muted " +
  "transition-colors duration-(--duration-fast) hover:text-fg focus-visible:outline-2 focus-visible:outline-ring";

export function Group({
  legend,
  hint,
  children,
}: {
  legend: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-2 text-sm font-medium text-fg">{legend}</legend>
      {children}
      {hint && <p className="text-xs text-fg-muted">{hint}</p>}
    </fieldset>
  );
}

/** Two number fields bounding one criterion, with the pair's error. */
export function Range({
  legend,
  labels,
  min,
  max,
  bounds,
  step,
  error,
  onChange,
}: {
  legend: string;
  labels: [string, string];
  min: string;
  max: string;
  bounds: [number, number];
  step?: number;
  error?: string;
  onChange: (min: string, max: string) => void;
}) {
  const errorId = useId();
  const field = (label: string, value: string, set: (v: string) => void) => (
    <Input
      label={label}
      type="number"
      inputMode={step && step < 1 ? "decimal" : "numeric"}
      min={bounds[0]}
      max={bounds[1]}
      step={step}
      value={value}
      aria-invalid={error ? true : undefined}
      aria-errormessage={error ? errorId : undefined}
      onChange={(event) => set(event.target.value)}
    />
  );
  return (
    <Group legend={legend}>
      <div className="grid grid-cols-2 gap-3">
        {field(labels[0], min, (v) => onChange(v, max))}
        {field(labels[1], max, (v) => onChange(min, v))}
      </div>
      {error && (
        <p id={errorId} role="alert" className="text-xs text-danger">
          {error}
        </p>
      )}
    </Group>
  );
}

/** A row of chips, each toggling one value in or out of `chosen`. */
export function Chips<T extends string>({
  options,
  chosen,
  onToggle,
}: {
  options: readonly { value: T; label: ReactNode }[];
  chosen: readonly T[];
  onToggle: (value: T) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {options.map(({ value, label }) => {
        const on = chosen.includes(value);
        return (
          <button
            key={value}
            type="button"
            aria-pressed={on}
            onClick={() => onToggle(value)}
            className={cn(
              chipClass,
              on && "border-accent bg-accent-soft text-fg",
            )}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}

/**
 * The genre chips and the field that adds one. The field's text is part of
 * the draft, so a genre typed and not yet added is still sent on submit.
 */
export function Genres({
  genres,
  text,
  onChange,
}: {
  genres: string[];
  text: string;
  onChange: (genres: string[], text: string) => void;
}) {
  const listId = useId();
  function add() {
    onChange(withGenre(genres, text), "");
  }
  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      add();
    } else if (event.key === "Backspace" && text === "" && genres.length) {
      onChange(genres.slice(0, -1), text);
    }
  }
  return (
    <Group
      legend="Genres"
      hint="A title with any of these passes. Press Enter to add one."
    >
      {genres.length > 0 && (
        <ul aria-label="Chosen genres" className="flex flex-wrap gap-2">
          {genres.map((genre) => (
            <li
              key={genre}
              className={cn(chipClass, "border-accent bg-accent-soft pr-1")}
            >
              <span className="text-fg">{genre}</span>
              <button
                type="button"
                aria-label={`Remove ${genre}`}
                onClick={() =>
                  onChange(
                    genres.filter((g) => g !== genre),
                    text,
                  )
                }
                className="inline-flex size-6 items-center justify-center rounded-full text-fg-muted hover:bg-surface-raised hover:text-fg focus-visible:outline-2 focus-visible:outline-ring"
              >
                <X aria-hidden className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex gap-2">
        <Input
          aria-label="Add a genre"
          placeholder="Drama, Animation…"
          list={listId}
          value={text}
          onChange={(event) => onChange(genres, event.target.value)}
          onKeyDown={onKeyDown}
        />
        <Button
          variant="secondary"
          disabled={text.trim() === ""}
          onClick={add}
          aria-label="Add genre"
        >
          <Plus aria-hidden className="size-4" />
        </Button>
      </div>
      <datalist id={listId}>
        {GENRE_SUGGESTIONS.filter((g) => !genres.includes(g)).map((g) => (
          <option key={g} value={g} />
        ))}
      </datalist>
    </Group>
  );
}
