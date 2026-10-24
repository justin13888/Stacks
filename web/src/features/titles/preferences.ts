// The signed-in user's display preferences, and the title each one shows.

import type { Preferences, TitleLanguage, TitleSummary } from "@/api/schemas";
import { useCurrentUser } from "@/hooks/use-auth";

/** What a page assumes before the session has loaded (it always has, under the guard). */
export const DEFAULT_PREFERENCES: Preferences = {
  score_format: "point_10_decimal",
  title_language: "english",
  spoilers: "hide",
  theme: "system",
};

export function usePreferences(): Preferences {
  return useCurrentUser()?.preferences ?? DEFAULT_PREFERENCES;
}

/**
 * A title in the user's title language. `title` is the English (or, when a
 * provider has none, the romanised) title, `original_title` the native one
 * and `romaji_title` the romanised one. A language the title has no name in
 * falls back to `title`.
 */
export function displayTitle(
  title: Pick<TitleSummary, "title" | "original_title" | "romaji_title">,
  language: TitleLanguage,
): string {
  if (language === "native") return title.original_title || title.title;
  if (language === "romaji") return title.romaji_title || title.title;
  return title.title;
}
