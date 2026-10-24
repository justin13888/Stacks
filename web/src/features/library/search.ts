// The Library's URL search params: the view, the filters and the order.
// They are the page's whole state, so a filtered view can be bookmarked,
// shared and restored with Back. Anything the URL holds that is not a value
// the API accepts is dropped rather than sent.

import { Category, type ListLibraryParams, Sort, Status } from "@/api/schemas";

export const LIBRARY_VIEWS = ["grid", "list", "board"] as const;
export type LibraryView = (typeof LIBRARY_VIEWS)[number];

export interface LibrarySearch {
  view?: LibraryView;
  status?: Status;
  category?: Category;
  favourite?: boolean;
  /** A tag id. */
  tag?: string;
  q?: string;
  sort?: Sort;
}

function oneOf<T extends string>(
  values: readonly T[],
  value: unknown,
): T | undefined {
  return typeof value === "string" &&
    (values as readonly string[]).includes(value)
    ? (value as T)
    : undefined;
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** TanStack Router's `validateSearch`: keeps only well-formed values. */
export function validateLibrarySearch(
  raw: Record<string, unknown>,
): LibrarySearch {
  // Every key is present, `undefined` when the URL's value is unusable: the
  // router layers a route's validated search over the raw one, so a key left
  // out would let the raw value through. `undefined` is never written to the
  // URL.
  return {
    view: oneOf(LIBRARY_VIEWS, raw.view),
    status: oneOf(Object.values(Status), raw.status),
    category: oneOf(Object.values(Category), raw.category),
    favourite:
      raw.favourite === true || raw.favourite === "true"
        ? true
        : raw.favourite === false || raw.favourite === "false"
          ? false
          : undefined,
    tag:
      typeof raw.tag === "string" && UUID.test(raw.tag) ? raw.tag : undefined,
    q:
      typeof raw.q === "string" && raw.q.trim() !== ""
        ? raw.q.trim().slice(0, 200)
        : undefined,
    sort: oneOf(Object.values(Sort), raw.sort),
  };
}

/** The listing request a search describes. */
export function libraryParams(
  search: LibrarySearch,
  overrides: Partial<ListLibraryParams> = {},
): ListLibraryParams {
  return {
    status: search.status,
    category: search.category,
    favourite: search.favourite,
    tag: search.tag,
    q: search.q,
    sort: search.sort,
    ...overrides,
  };
}
