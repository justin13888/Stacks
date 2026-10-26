// A library entry's data: reading it, the caches a change to it touches, and
// the multi-call operations (add, log the next episode) that the title
// page, the Library and Up Next share. Every call is
// an orval-generated operation.

import { type QueryClient, queryOptions } from "@tanstack/react-query";
import {
  getGetSeasonQueryOptions,
  getGetTitleQueryKey,
  getListSeasonsQueryOptions,
} from "@/api/catalog/catalog";
import {
  getGetLibraryEntryQueryKey,
  getLibraryEntry,
  getListCalendarQueryKey,
  getListEntryEventsQueryKey,
  getListHistoryQueryKey,
  getListLibraryQueryKey,
  getListUpNextQueryKey,
  logPlays,
  putLibraryEntry,
  restoreLibraryEntry,
  updateLibraryEntry,
} from "@/api/library/library";
import type {
  LibraryEntryDetail,
  LoggedPlays,
  Status,
  TitleRef,
  WatchRun,
} from "@/api/schemas";
import { isCatalogListing } from "@/features/blocklist/use-blocklist";
import { type EpisodeRef, nextEpisode } from "@/features/titles/episodes";
import { isApiProblem } from "@/lib/api-client";

/**
 * The caller's entry for a title, or `null` when the title is not in the
 * library (a 404 is an answer here, not a failure).
 */
export function entryQueryOptions(ref: TitleRef) {
  return queryOptions({
    queryKey: getGetLibraryEntryQueryKey(ref),
    queryFn: async ({ signal }): Promise<LibraryEntryDetail | null> => {
      try {
        return await getLibraryEntry(ref, { signal });
      } catch (error) {
        if (isApiProblem(error) && error.status === 404) return null;
        throw error;
      }
    },
  });
}

/** The current run's number: the last run, or 1 before any. */
export function currentRun(runs: readonly WatchRun[]): number {
  return runs.reduce((max, run) => Math.max(max, run.number), 1);
}

/**
 * Refetches everything a change to `ref`'s entry can alter: the entry, the
 * title's library state, every listing that shows entries, and every
 * catalog listing (feeds, search, recommendations), whose cards show each
 * title's library state.
 */
export async function invalidateTracking(
  queryClient: QueryClient,
  ref?: TitleRef,
): Promise<void> {
  const keys: readonly (readonly unknown[])[] = [
    getListLibraryQueryKey(),
    getListUpNextQueryKey(),
    getListCalendarQueryKey(),
    getListHistoryQueryKey(),
    ...(ref
      ? [
          getGetLibraryEntryQueryKey(ref),
          getGetTitleQueryKey(ref),
          getListEntryEventsQueryKey(ref),
        ]
      : []),
  ];
  await Promise.all([
    ...keys.map((queryKey) => queryClient.invalidateQueries({ queryKey })),
    queryClient.invalidateQueries({
      predicate: (query) => isCatalogListing(query.queryKey),
    }),
  ]);
}

/**
 * What an add came to: the title was `created`, `restored` from the trash,
 * or already in the library, where its entry was `kept` exactly as it was.
 */
export type AddOutcome = "created" | "restored" | "kept";

/** Makes `putLibraryEntry` create-only (RFC 9110 section 13.1.2). */
const CREATE_ONLY = { "If-None-Match": "*" };

/** Whether the server refused an add because the title is in the library. */
function isAlreadyInLibrary(error: unknown): boolean {
  return (
    isApiProblem(error) &&
    (error.kind === "entry-live" || error.kind === "entry-exists")
  );
}

/**
 * Adds a title with `status`, never touching an entry already in the
 * library (a stale card, a double click, a second tab): restore answers 409
 * for one, and the create is create-only (`If-None-Match: *`, a 412 for
 * one). A title in the trash is restored as it was (score, notes and
 * favourite intact) rather than recreated empty, then moved to `status`
 * unless `keepRestoredStatus`.
 */
export async function addToLibrary(
  ref: TitleRef,
  status: Status,
  { keepRestoredStatus = false } = {},
): Promise<AddOutcome> {
  try {
    const restored = await restoreLibraryEntry(ref);
    if (!keepRestoredStatus && restored.status !== status) {
      await updateLibraryEntry(ref, { status });
    }
    return "restored";
  } catch (error) {
    if (isAlreadyInLibrary(error)) return "kept";
    if (!isApiProblem(error) || error.status !== 404) throw error;
  }
  try {
    await putLibraryEntry(ref, { status }, { headers: CREATE_ONLY });
    return "created";
  } catch (error) {
    if (isAlreadyInLibrary(error)) return "kept";
    throw error;
  }
}

/**
 * The episode "+1" logs for `ref`, reading the entry, the seasons and the
 * season details it needs through the query cache. `null` when the show has
 * nothing left.
 */
export async function resolveNextEpisode(
  queryClient: QueryClient,
  ref: TitleRef,
): Promise<EpisodeRef | null> {
  const [entry, seasons] = await Promise.all([
    queryClient.fetchQuery(entryQueryOptions(ref)),
    queryClient.ensureQueryData(getListSeasonsQueryOptions(ref)),
  ]);
  return nextEpisode(seasons.items, entry?.watched ?? [], async (season) => {
    const detail = await queryClient.ensureQueryData(
      getGetSeasonQueryOptions(ref, season),
    );
    return detail.episodes.map((e) => e.number);
  });
}

/** Logs one episode of `ref` (or the movie, when `episode` is `null`). */
export function logEpisode(
  ref: TitleRef,
  episode: EpisodeRef | null,
): Promise<LoggedPlays> {
  return logPlays(
    ref,
    episode
      ? {
          kind: "episodes",
          episodes: [{ season: episode.season, episode: episode.episode }],
        }
      : { kind: "movie" },
  );
}
