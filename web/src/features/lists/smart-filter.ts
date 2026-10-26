// A smart list's filter as the form edits it: every criterion the API's
// `ListFilter` takes, held as the fields show it (numbers as typed text),
// and the conversions both ways. A criterion left empty is left out of the
// filter, so it does not narrow the list.

import type { Kind, ListFilter, SmartSort, Status } from "@/api/schemas";
import { STATUS_LABELS } from "@/components/ui/status-badge";

export type KindChoice = Kind | "both";

export interface FilterDraft {
  sort: SmartSort;
  kinds: KindChoice;
  genres: string[];
  /** A genre typed into the field and not yet added as a chip. */
  genreText: string;
  statuses: Status[];
  yearMin: string;
  yearMax: string;
  ratingMin: string;
  ratingMax: string;
}

export const SORT_LABELS: Record<SmartSort, string> = {
  popular: "Popular",
  trending: "Trending",
  top_rated: "Top rated",
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

/** The form's fields for a stored filter; the empty filter when none. */
export function draftFromFilter(filter?: ListFilter | null): FilterDraft {
  const kinds = filter?.kinds ?? [];
  const text = (value?: number | null) => (value == null ? "" : String(value));
  return {
    sort: filter?.sort ?? DEFAULT_SORT,
    kinds: kinds.length === 1 && kinds[0] ? kinds[0] : "both",
    genres: [...(filter?.genres ?? [])],
    genreText: "",
    statuses: [...(filter?.statuses ?? [])],
    yearMin: text(filter?.year_min),
    yearMax: text(filter?.year_max),
    ratingMin: text(filter?.rating_min),
    ratingMax: text(filter?.rating_max),
  };
}

/** `genres` with the typed `text` added, unless it is empty or already there. */
export function withGenre(genres: string[], text: string): string[] {
  const genre = text.trim().replace(/,+$/, "").trim();
  const known = genres.some((g) => g.toLowerCase() === genre.toLowerCase());
  return genre && !known ? [...genres, genre] : genres;
}

export interface FilterErrors {
  year?: string;
  rating?: string;
}

function whole(text: string): number | undefined | null {
  const trimmed = text.trim();
  if (trimmed === "") return undefined;
  return /^-?\d+$/.test(trimmed) ? Number(trimmed) : null;
}

/** A pair of bounds, or the message saying why it is unusable. */
function range(
  min: string,
  max: string,
  [floor, ceiling]: [number, number],
  noun: string,
  unusable: string,
): { min?: number; max?: number } | string {
  const lo = whole(min);
  const hi = whole(max);
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
  if (typeof years === "string" || typeof rating === "string") {
    return {
      errors: {
        year: typeof years === "string" ? years : undefined,
        rating: typeof rating === "string" ? rating : undefined,
      },
    };
  }
  const filter: ListFilter = { sort: draft.sort };
  if (draft.kinds !== "both") filter.kinds = [draft.kinds];
  // A genre still in the field counts too, as if it had been added.
  const genres = withGenre(draft.genres, draft.genreText);
  if (genres.length > 0) filter.genres = genres;
  if (draft.statuses.length > 0) filter.statuses = draft.statuses;
  if (years.min !== undefined) filter.year_min = years.min;
  if (years.max !== undefined) filter.year_max = years.max;
  if (rating.min !== undefined) filter.rating_min = rating.min;
  if (rating.max !== undefined) filter.rating_max = rating.max;
  return { filter };
}

function bounds(
  min: number | null | undefined,
  max: number | null | undefined,
  [only, from, to]: [string, string, string],
): string | undefined {
  if (min != null && max != null) {
    return min === max ? `${only} ${min}` : `${only} ${min}–${max}`;
  }
  if (min != null) return `${from} ${min}`;
  if (max != null) return `${to} ${max}`;
  return undefined;
}

/** A filter in a few words each, for a smart list's summary. */
export function describeFilter(filter?: ListFilter | null): string[] {
  const draft = draftFromFilter(filter);
  return [
    `${SORT_LABELS[draft.sort]} ${KIND_LABELS[draft.kinds].replace(" only", "").toLowerCase()}`,
    filter?.genres?.length ? filter.genres.join(" or ") : undefined,
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
  ].filter((part): part is string => part !== undefined);
}
