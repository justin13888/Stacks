// A smart list's filter as the form edits it: every criterion the API's
// `ListFilter` takes, held as the fields show it (numbers as typed text),
// and the conversions both ways. A criterion left empty is left out of the
// filter, so it does not narrow the list.

import type {
  Category,
  Kind,
  ListFilter,
  ScoreFormat,
  SmartSort,
  Status,
} from "@/api/schemas";
import { CATEGORY_NAMES } from "@/components/search/categories";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import {
  formatScore,
  fromDisplayScore,
  maxScore,
  toDisplayScore,
} from "@/lib/score-format";

export type KindChoice = Kind | "both";

export interface FilterDraft {
  sort: SmartSort;
  kinds: KindChoice;
  genres: string[];
  /** A genre typed into the field and not yet added as a chip. */
  genreText: string;
  categories: Category[];
  statuses: Status[];
  /** Tag ids, including any deleted since the filter was written. */
  tags: string[];
  yearMin: string;
  yearMax: string;
  ratingMin: string;
  ratingMax: string;
  /** The user's own score bounds, in their score format's units. */
  scoreMin: string;
  scoreMax: string;
  scoreFormat: ScoreFormat;
  /**
   * The stored score bounds the draft was loaded with, and their text: a
   * bound left as loaded is sent as it was, so a format coarser than the
   * stored 0–100 never rounds it.
   */
  loadedScores: {
    min?: number;
    max?: number;
    minText: string;
    maxText: string;
  };
}

export const SORT_LABELS: Record<SmartSort, string> = {
  popular: "Popular",
  trending: "Trending",
  top_rated: "Top rated",
  library: "Your library",
};

export const KIND_LABELS: Record<KindChoice, string> = {
  both: "Movies and shows",
  movie: "Movies only",
  show: "Shows only",
};

/** The API's bounds on a release year: any `int32`. */
export const YEAR_BOUNDS: [number, number] = [-(2 ** 31), 2 ** 31 - 1];

/** The API's bounds on a provider rating. */
export const RATING_BOUNDS: [number, number] = [0, 10];

/** The feed the server draws from when a filter names none. */
const DEFAULT_SORT: SmartSort = "popular";

/**
 * The form's fields for a stored filter; the empty filter when none. Scores
 * are shown in `scoreFormat`, the user's own.
 */
export function draftFromFilter(
  filter?: ListFilter | null,
  scoreFormat: ScoreFormat = "point_100",
): FilterDraft {
  const kinds = filter?.kinds ?? [];
  const text = (value?: number | null) => (value == null ? "" : String(value));
  const score = (value?: number | null) =>
    value == null ? "" : String(toDisplayScore(value, scoreFormat));
  return {
    sort: filter?.sort ?? DEFAULT_SORT,
    kinds: kinds.length === 1 && kinds[0] ? kinds[0] : "both",
    genres: [...(filter?.genres ?? [])],
    genreText: "",
    categories: [...(filter?.categories ?? [])],
    statuses: [...(filter?.statuses ?? [])],
    tags: [...(filter?.tags ?? [])],
    yearMin: text(filter?.year_min),
    yearMax: text(filter?.year_max),
    ratingMin: text(filter?.rating_min),
    ratingMax: text(filter?.rating_max),
    scoreMin: score(filter?.score_min),
    scoreMax: score(filter?.score_max),
    scoreFormat,
    loadedScores: {
      min: filter?.score_min ?? undefined,
      max: filter?.score_max ?? undefined,
      minText: score(filter?.score_min),
      maxText: score(filter?.score_max),
    },
  };
}

/** `genres` with the typed `text` added, unless it is empty or already there. */
export function withGenre(genres: string[], text: string): string[] {
  const genre = text.trim().replace(/,+$/, "").trim();
  const known = genres.some((g) => g.toLowerCase() === genre.toLowerCase());
  return genre && !known ? [...genres, genre] : genres;
}

/** `items` with `item` added, or taken out when it is there. */
export function toggled<T>(items: readonly T[], item: T): T[] {
  return items.includes(item)
    ? items.filter((i) => i !== item)
    : [...items, item];
}

export interface FilterErrors {
  year?: string;
  rating?: string;
  score?: string;
}

/** A number as typed: `undefined` when empty, `null` when not one. */
function parsed(text: string, decimals: boolean): number | undefined | null {
  const trimmed = text.trim();
  if (trimmed === "") return undefined;
  const pattern = decimals ? /^-?\d+(\.\d)?$/ : /^-?\d+$/;
  return pattern.test(trimmed) ? Number(trimmed) : null;
}

/** A pair of bounds, or the message saying why it is unusable. */
function range(
  min: string,
  max: string,
  [floor, ceiling]: [number, number],
  noun: string,
  unusable: string,
  decimals = false,
): { min?: number; max?: number } | string {
  const lo = parsed(min, decimals);
  const hi = parsed(max, decimals);
  const outside = (n: number | undefined | null) =>
    n === null || (n !== undefined && (n < floor || n > ceiling));
  if (outside(lo) || outside(hi)) {
    return unusable;
  }
  if (lo != null && hi != null && lo > hi) {
    return `The first ${noun} must not be after the second.`;
  }
  return { min: lo ?? undefined, max: hi ?? undefined };
}

/** The user's score bounds as stored (0–100), or why they are unusable. */
function scores(draft: FilterDraft): { min?: number; max?: number } | string {
  const format = draft.scoreFormat;
  const top = maxScore(format);
  const read = range(
    draft.scoreMin,
    draft.scoreMax,
    [0, top],
    "score",
    `Use a score from 0 to ${top}.`,
    format === "point_10_decimal",
  );
  if (typeof read === "string") return read;
  const { loadedScores: loaded } = draft;
  const stored = (
    value: number | undefined,
    text: string,
    loadedText: string,
    was?: number,
  ) => {
    if (was !== undefined && text.trim() === loadedText) return was;
    return value === undefined ? undefined : fromDisplayScore(value, format);
  };
  return {
    min: stored(read.min, draft.scoreMin, loaded.minText, loaded.min),
    max: stored(read.max, draft.scoreMax, loaded.maxText, loaded.max),
  };
}

/** The filter the fields describe, or what is wrong with them. */
export function filterFromDraft(
  draft: FilterDraft,
): { filter: ListFilter } | { errors: FilterErrors } {
  const years = range(
    draft.yearMin,
    draft.yearMax,
    YEAR_BOUNDS,
    "year",
    "Use a whole year.",
  );
  const rating = range(
    draft.ratingMin,
    draft.ratingMax,
    RATING_BOUNDS,
    "rating",
    "Use a whole rating from 0 to 10.",
  );
  const score = scores(draft);
  if (
    typeof years === "string" ||
    typeof rating === "string" ||
    typeof score === "string"
  ) {
    const error = (r: object | string) =>
      typeof r === "string" ? r : undefined;
    return {
      errors: {
        year: error(years),
        rating: error(rating),
        score: error(score),
      },
    };
  }
  const filter: ListFilter = { sort: draft.sort };
  if (draft.kinds !== "both") filter.kinds = [draft.kinds];
  // A genre still in the field counts too, as if it had been added.
  const genres = withGenre(draft.genres, draft.genreText);
  if (genres.length > 0) filter.genres = genres;
  if (draft.categories.length > 0) filter.categories = draft.categories;
  if (draft.statuses.length > 0) filter.statuses = draft.statuses;
  if (draft.tags.length > 0) filter.tags = draft.tags;
  if (years.min !== undefined) filter.year_min = years.min;
  if (years.max !== undefined) filter.year_max = years.max;
  if (rating.min !== undefined) filter.rating_min = rating.min;
  if (rating.max !== undefined) filter.rating_max = rating.max;
  if (score.min !== undefined) filter.score_min = score.min;
  if (score.max !== undefined) filter.score_max = score.max;
  return { filter };
}

function bounds(
  min: number | null | undefined,
  max: number | null | undefined,
  [only, from, to]: [string, string, string],
  show: (n: number) => string = String,
): string | undefined {
  if (min != null && max != null) {
    return min === max
      ? `${only} ${show(min)}`
      : `${only} ${show(min)}–${show(max)}`;
  }
  if (min != null) return `${from} ${show(min)}`;
  if (max != null) return `${to} ${show(max)}`;
  return undefined;
}

/** What the summary needs beyond the filter: the user's score format and tag names. */
export interface DescribeContext {
  scoreFormat?: ScoreFormat;
  /** Tag names by id; an id it lacks names a deleted tag. */
  tagNames?: ReadonlyMap<string, string>;
}

/** A filter in a few words each, for a smart list's summary. */
export function describeFilter(
  filter?: ListFilter | null,
  { scoreFormat = "point_100", tagNames }: DescribeContext = {},
): string[] {
  const draft = draftFromFilter(filter);
  const kinds = KIND_LABELS[draft.kinds].replace(" only", "");
  const source =
    draft.sort === "library"
      ? `${kinds} from your library`
      : `${SORT_LABELS[draft.sort]} ${kinds.toLowerCase()}`;
  const tags = filter?.tags?.map((id) => tagNames?.get(id) ?? "a deleted tag");
  return [
    source,
    filter?.genres?.length ? filter.genres.join(" or ") : undefined,
    filter?.categories?.length
      ? filter.categories.map((c) => CATEGORY_NAMES[c]).join(" or ")
      : undefined,
    bounds(filter?.year_min, filter?.year_max, [
      "Released",
      "Released since",
      "Released by",
    ]),
    bounds(filter?.rating_min, filter?.rating_max, [
      "Rated",
      "Rated at least",
      "Rated at most",
    ]),
    filter?.statuses?.length
      ? `In your library as ${filter.statuses.map((s) => STATUS_LABELS[s]).join(" or ")}`
      : undefined,
    tags?.length ? `Tagged ${tags.join(" or ")}` : undefined,
    bounds(
      filter?.score_min,
      filter?.score_max,
      ["You scored", "You scored at least", "You scored at most"],
      (n) => formatScore(n, scoreFormat),
    ),
  ].filter((part): part is string => part !== undefined);
}
