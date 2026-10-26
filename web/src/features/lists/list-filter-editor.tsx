import { Plus, Sparkles, X } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useId } from "react";
import { type ListFilter, SmartSort, Status } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import { cn } from "@/lib/cn";
import {
  describeFilter,
  type FilterDraft,
  type FilterErrors,
  KIND_LABELS,
  type KindChoice,
  RATING_BOUNDS,
  SORT_LABELS,
  withGenre,
  YEAR_BOUNDS,
} from "./smart-filter";

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

const chipClass =
  "inline-flex h-8 items-center gap-1.5 rounded-full border border-line bg-surface px-3 text-sm text-fg-muted " +
  "transition-colors duration-(--duration-fast) hover:text-fg focus-visible:outline-2 focus-visible:outline-ring";

function Group({
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
function Range({
  legend,
  labels,
  min,
  max,
  bounds,
  error,
  onChange,
}: {
  legend: string;
  labels: [string, string];
  min: string;
  max: string;
  bounds: [number, number];
  error?: string;
  onChange: (min: string, max: string) => void;
}) {
  const errorId = useId();
  const field = (label: string, value: string, set: (v: string) => void) => (
    <Input
      label={label}
      type="number"
      inputMode="numeric"
      min={bounds[0]}
      max={bounds[1]}
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

/**
 * The genre chips and the field that adds one. The field's text is part of
 * the draft, so a genre typed and not yet added is still sent on submit.
 */
function Genres({
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

/** Every criterion a smart list's filter takes, as form fields. */
export function ListFilterEditor({
  draft,
  errors,
  onChange,
}: {
  draft: FilterDraft;
  errors: FilterErrors;
  onChange: (draft: FilterDraft) => void;
}) {
  const set = (patch: Partial<FilterDraft>) => onChange({ ...draft, ...patch });
  return (
    <div className="flex flex-col gap-5">
      <div className="grid gap-3 sm:grid-cols-2">
        <Select<SmartSort>
          label="Draw from"
          value={draft.sort}
          options={Object.values(SmartSort).map((s) => ({
            value: s,
            label: SORT_LABELS[s],
          }))}
          onValueChange={(sort) => set({ sort })}
        />
        <Select<KindChoice>
          label="Kind"
          value={draft.kinds}
          options={(["both", "movie", "show"] as const).map((k) => ({
            value: k,
            label: KIND_LABELS[k],
          }))}
          onValueChange={(kinds) => set({ kinds })}
        />
      </div>
      <Genres
        genres={draft.genres}
        text={draft.genreText}
        onChange={(genres, genreText) => set({ genres, genreText })}
      />
      <Range
        legend="Release year"
        labels={["From year", "To year"]}
        min={draft.yearMin}
        max={draft.yearMax}
        bounds={YEAR_BOUNDS}
        error={errors.year}
        onChange={(yearMin, yearMax) => set({ yearMin, yearMax })}
      />
      <Range
        legend="Rating out of 10"
        labels={["At least", "At most"]}
        min={draft.ratingMin}
        max={draft.ratingMax}
        bounds={RATING_BOUNDS}
        error={errors.rating}
        onChange={(ratingMin, ratingMax) => set({ ratingMin, ratingMax })}
      />
      <Group
        legend="In your library as"
        hint="With none chosen, titles you have not tracked pass too."
      >
        <div className="flex flex-wrap gap-2">
          {Object.values(Status).map((status) => {
            const on = draft.statuses.includes(status);
            return (
              <button
                key={status}
                type="button"
                aria-pressed={on}
                onClick={() =>
                  set({
                    statuses: on
                      ? draft.statuses.filter((s) => s !== status)
                      : [...draft.statuses, status],
                  })
                }
                className={cn(
                  chipClass,
                  on && "border-accent bg-accent-soft text-fg",
                )}
              >
                {STATUS_LABELS[status]}
              </button>
            );
          })}
        </div>
      </Group>
    </div>
  );
}

/** What a smart list's filter picks, in a line of chips. */
export function SmartFilterSummary({ filter }: { filter?: ListFilter | null }) {
  return (
    <div className="mb-4 flex flex-col gap-2 text-sm text-fg-muted">
      <p className="flex items-center gap-1.5">
        <Sparkles aria-hidden className="size-4 text-accent" />A smart list: its
        filter picks the titles, in its feed's order.
      </p>
      <ul aria-label="Its filter" className="flex flex-wrap gap-2">
        {describeFilter(filter).map((part) => (
          <li
            key={part}
            className="rounded-full bg-surface-sunken px-2.5 py-1 text-xs text-fg"
          >
            {part}
          </li>
        ))}
      </ul>
    </div>
  );
}
