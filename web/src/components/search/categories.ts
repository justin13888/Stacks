import type { Category } from "@/api/schemas";

/** Every browsing category the API derives, with its display name. */
export const CATEGORY_LABELS: Record<Category, string> = {
  movie: "Movies",
  series: "Series",
  anime: "Anime",
  "k-drama": "K-drama",
  "j-drama": "J-drama",
  "c-drama": "C-drama",
  "tw-drama": "TW-drama",
  "th-drama": "Thai drama",
};

/** The singular label shown beside one title. */
export const CATEGORY_NAMES: Record<Category, string> = {
  movie: "Movie",
  series: "Series",
  anime: "Anime",
  "k-drama": "K-drama",
  "j-drama": "J-drama",
  "c-drama": "C-drama",
  "tw-drama": "TW-drama",
  "th-drama": "Thai drama",
};

/** The chip order in the palette. */
export const CATEGORIES: readonly Category[] = [
  "movie",
  "series",
  "anime",
  "k-drama",
  "j-drama",
  "c-drama",
  "tw-drama",
  "th-drama",
];
