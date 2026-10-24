// The Discover rails: which feed each one reads, and its heading.

import { Feed, Kind } from "@/api/schemas";

export interface Rail {
  feed: Feed;
  /** Movies or shows; the feed's own default when absent. */
  kind?: Kind;
  title: string;
}

/** Every rail, in page order: general first, then anime, then dramas. */
export const RAILS: readonly Rail[] = [
  { feed: "trending", kind: "movie", title: "Trending movies" },
  { feed: "trending", kind: "show", title: "Trending shows" },
  { feed: "popular", kind: "movie", title: "Popular movies" },
  { feed: "popular", kind: "show", title: "Popular shows" },
  { feed: "top-rated", kind: "movie", title: "Top rated movies" },
  { feed: "top-rated", kind: "show", title: "Top rated shows" },
  { feed: "airing-anime", title: "Airing anime" },
  { feed: "trending-anime", title: "Trending anime" },
  { feed: "popular-anime", title: "Popular anime" },
  { feed: "top-rated-anime", title: "Top rated anime" },
  { feed: "trending-anime", kind: "movie", title: "Anime films" },
  { feed: "k-drama", title: "K-drama" },
  { feed: "j-drama", title: "J-drama" },
  { feed: "c-drama", title: "C-drama" },
  { feed: "tw-drama", title: "TW-drama" },
  { feed: "th-drama", title: "Thai drama" },
];

const FEEDS = Object.values(Feed);

export function isFeed(value: string): value is Feed {
  return (FEEDS as string[]).includes(value);
}

/** A feed's heading on its "see all" page. */
export function railTitle(feed: Feed, kind: Kind | undefined): string {
  const rail =
    RAILS.find((r) => r.feed === feed && r.kind === kind) ??
    RAILS.find((r) => r.feed === feed && r.kind === undefined) ??
    RAILS.find((r) => r.feed === feed);
  return rail?.title ?? feed;
}

/** The see-all page's search params: only a kind the API knows. */
export function validateFeedSearch(raw: Record<string, unknown>): {
  kind?: Kind;
} {
  const kind =
    typeof raw.kind === "string" &&
    (Object.values(Kind) as string[]).includes(raw.kind)
      ? (raw.kind as Kind)
      : undefined;
  return { kind };
}
