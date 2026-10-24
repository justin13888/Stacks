// Episode arithmetic for the seasons grid, "+1 episode" and "mark up to
// here". Pure functions over what the API returns; nothing here fetches.

import type { Episode, EpisodeNumber, NewPlays, Season } from "@/api/schemas";

/** An episode by its place in a season (a `WatchedEpisode` is one). */
export interface EpisodeRef {
  season: number;
  episode: number;
}

export function episodeKey(season: number, episode: number): string {
  return `${season}:${episode}`;
}

/** The episodes watched in the current run, as keys. */
export function watchedKeys(watched: readonly EpisodeRef[]): Set<string> {
  return new Set(watched.map((w) => episodeKey(w.season, w.episode)));
}

/** `S2E5`, or `Special 3` for season 0. */
export function episodeLabel({ season, episode }: EpisodeRef): string {
  return season === 0 ? `Special ${episode}` : `S${season}E${episode}`;
}

/**
 * Season tabs in viewing order: regular seasons ascending, then specials
 * (season 0) last.
 */
export function orderSeasons(seasons: readonly Season[]): Season[] {
  return [...seasons].sort((a, b) => {
    if (a.number === 0) return 1;
    if (b.number === 0) return -1;
    return a.number - b.number;
  });
}

/** The regular season numbers (no specials), ascending. */
export function regularSeasons(seasons: readonly Season[]): number[] {
  return seasons
    .map((s) => s.number)
    .filter((n) => n > 0)
    .sort((a, b) => a - b);
}

function compare(a: EpisodeRef, b: EpisodeRef): number {
  return a.season - b.season || a.episode - b.episode;
}

/** The furthest regular (non-special) episode watched in the current run. */
export function furthestWatched(
  watched: readonly EpisodeRef[],
): EpisodeRef | null {
  let furthest: EpisodeRef | null = null;
  for (const w of watched) {
    if (w.season === 0) continue;
    const ref = { season: w.season, episode: w.episode };
    if (!furthest || compare(ref, furthest) > 0) furthest = ref;
  }
  return furthest;
}

/**
 * The episode "+1" logs: the one after the furthest regular episode watched
 * in the current run (the first, when none is), crossing into the next
 * season when a season is done. This is the same episode Up Next offers.
 * Specials are never "next"; they are logged from their own tab.
 *
 * `episodesOf` gives a season's episode numbers; it is asked only for the
 * seasons the answer depends on, in order, so a caller can load them lazily.
 */
export async function nextEpisode(
  seasons: readonly Season[],
  watched: readonly EpisodeRef[],
  episodesOf: (season: number) => Promise<readonly number[]>,
): Promise<EpisodeRef | null> {
  const furthest = furthestWatched(watched);
  for (const season of regularSeasons(seasons)) {
    if (furthest && season < furthest.season) continue;
    const numbers = [...(await episodesOf(season))].sort((a, b) => a - b);
    const next = numbers.find(
      (n) => !furthest || season > furthest.season || n > furthest.episode,
    );
    if (next !== undefined) return { season, episode: next };
  }
  return null;
}

/**
 * The plays "mark up to here" logs, as one request. A regular episode uses
 * the server's `through` (every unwatched episode from S1E1 up to it); a
 * special lists the unwatched specials up to it, as `through` covers regular
 * seasons only. `null` when there is nothing left to log.
 */
export function markThroughPlays(
  target: EpisodeRef,
  seasonEpisodes: readonly Episode[],
  watched: ReadonlySet<string>,
): NewPlays | null {
  if (target.season > 0) {
    return { kind: "through", season: target.season, episode: target.episode };
  }
  const episodes: EpisodeNumber[] = seasonEpisodes
    .filter(
      (e) =>
        e.number <= target.episode && !watched.has(episodeKey(0, e.number)),
    )
    .map((e) => ({ season: 0, episode: e.number }));
  return episodes.length > 0 ? { kind: "episodes", episodes } : null;
}

/**
 * The episodes a `through` request will mark, given what the grid knows:
 * used to show them watched before the server answers.
 */
export function throughEpisodes(
  target: EpisodeRef,
  seasons: readonly Season[],
  loaded: (season: number) => readonly Episode[] | undefined,
): EpisodeRef[] {
  const refs: EpisodeRef[] = [];
  for (const season of regularSeasons(seasons)) {
    if (season > target.season) break;
    for (const e of loaded(season) ?? []) {
      if (season === target.season && e.number > target.episode) continue;
      refs.push({ season, episode: e.number });
    }
  }
  return refs;
}
