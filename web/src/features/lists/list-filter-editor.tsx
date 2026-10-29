import { Sparkles } from "lucide-react";
import { type ListFilter, SmartSort, Status } from "@/api/schemas";
import { CATEGORIES, CATEGORY_NAMES } from "@/components/search/categories";
import { Select } from "@/components/ui/select";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import { TagSwatch } from "@/features/tags/tag-chip";
import { useAllTags } from "@/features/tags/use-tags";
import { usePreferences } from "@/features/titles/preferences";
import { maxScore } from "@/lib/score-format";
import { Chips, Genres, Group, Range } from "./filter-fields";
import {
  describeFilter,
  type FilterDraft,
  type FilterErrors,
  KIND_LABELS,
  type KindChoice,
  RATING_BOUNDS,
  SORT_LABELS,
  toggled,
  YEAR_BOUNDS,
} from "./smart-filter";

/** The tag chips: the caller's tags, and any the draft names that are gone. */
function Tags({
  chosen,
  onToggle,
}: {
  chosen: string[];
  onToggle: (id: string) => void;
}) {
  const tags = useAllTags().data?.items ?? [];
  const known = new Set(tags.map((t) => t.id));
  const options = [
    ...tags.map((tag) => ({
      value: tag.id,
      label: (
        <>
          <TagSwatch color={tag.color} />
          {tag.name}
        </>
      ),
    })),
    ...chosen
      .filter((id) => !known.has(id))
      .map((id) => ({ value: id, label: "Deleted tag" })),
  ];
  return (
    <Group
      legend="Tagged"
      hint={
        options.length === 0
          ? "You have no tags yet."
          : "A title in your library with any of these passes."
      }
    >
      {options.length > 0 && (
        <Chips options={options} chosen={chosen} onToggle={onToggle} />
      )}
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
  const top = maxScore(draft.scoreFormat);
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
      <Group legend="Categories" hint="A title in any of these passes.">
        <Chips
          options={CATEGORIES.map((c) => ({
            value: c,
            label: CATEGORY_NAMES[c],
          }))}
          chosen={draft.categories}
          onToggle={(c) => set({ categories: toggled(draft.categories, c) })}
        />
      </Group>
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
        <Chips
          options={Object.values(Status).map((s) => ({
            value: s,
            label: STATUS_LABELS[s],
          }))}
          chosen={draft.statuses}
          onToggle={(s) => set({ statuses: toggled(draft.statuses, s) })}
        />
      </Group>
      <Tags
        chosen={draft.tags}
        onToggle={(id) => set({ tags: toggled(draft.tags, id) })}
      />
      <Range
        legend={`Your score out of ${top}`}
        labels={["Scored at least", "Scored at most"]}
        min={draft.scoreMin}
        max={draft.scoreMax}
        bounds={[0, top]}
        step={draft.scoreFormat === "point_10_decimal" ? 0.1 : 1}
        error={errors.score}
        onChange={(scoreMin, scoreMax) => set({ scoreMin, scoreMax })}
      />
    </div>
  );
}

/** What a smart list's filter picks, in a line of chips. */
export function SmartFilterSummary({ filter }: { filter?: ListFilter | null }) {
  const { score_format } = usePreferences();
  const tags = useAllTags().data?.items ?? [];
  const tagNames = new Map(tags.map((t) => [t.id, t.name]));
  return (
    <div className="mb-4 flex flex-col gap-2 text-sm text-fg-muted">
      <p className="flex items-center gap-1.5">
        <Sparkles aria-hidden className="size-4 text-accent" />A smart list: its
        filter picks the titles, in its source's order.
      </p>
      <ul aria-label="Its filter" className="flex flex-wrap gap-2">
        {describeFilter(filter, { scoreFormat: score_format, tagNames }).map(
          (part) => (
            <li
              key={part}
              className="rounded-full bg-surface-sunken px-2.5 py-1 text-xs text-fg"
            >
              {part}
            </li>
          ),
        )}
      </ul>
    </div>
  );
}
