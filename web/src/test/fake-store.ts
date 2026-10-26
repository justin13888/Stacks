// The state behind the fake tracking API in test/fake-api: titles, seasons,
// one library entry per title with its runs and plays, and the entry event
// log, plus the response bodies derived from them. Test-only: no page
// imports it.

import type {
  Change,
  EntryEvent,
  Episode,
  LibraryEntry,
  LibraryEntryDetail,
  NewPlays,
  Play,
  Season,
  Status,
  Tag,
  TitleDetail,
  TitleSummary,
  WatchedEpisode,
  WatchRun,
} from "@/api/schemas";

export function makeTitle(
  overrides: Partial<TitleDetail> & Pick<TitleDetail, "ref" | "title">,
): TitleDetail {
  return {
    kind: "show",
    category: "series",
    genres: [],
    year: 2020,
    library: null,
    freshness: { stale: false },
    ...overrides,
  };
}

export function makeEpisodes(season: number, count: number): Episode[] {
  return Array.from({ length: count }, (_, i) => ({
    season,
    number: i + 1,
    name: `Episode ${season}.${i + 1}`,
    air_date: `2020-0${Math.min(season + 1, 9)}-${String(i + 1).padStart(2, "0")}`,
  }));
}

interface StoredEntry {
  status: Status;
  score: number | null;
  favourite: boolean;
  notes: string | null;
  runs: WatchRun[];
  plays: Play[];
  events: EntryEvent[];
  tags: Tag[];
}

export interface StoreSeed {
  titles: TitleDetail[];
  /** Seasons by title ref, each with its episodes. */
  seasons?: Record<string, { season: Season; episodes: Episode[] }[]>;
  /** Entries already in the library, by title ref. */
  entries?: Record<
    string,
    Partial<Omit<StoredEntry, "plays" | "events">> & {
      watched?: { season: number; episode: number }[];
    }
  >;
}

let sequence = 0;
function nextId(): string {
  sequence += 1;
  return `0190a8e0-0000-7000-8000-${String(sequence).padStart(12, "0")}`;
}

export const NOW = "2026-10-20T12:00:00Z";

export function createStore(seed: StoreSeed) {
  const titles = new Map(seed.titles.map((t) => [t.ref, t]));
  const entries = new Map<string, StoredEntry>();

  function record(entry: StoredEntry, change: Change) {
    entry.events.unshift({ id: nextId(), at: NOW, change });
  }

  function newPlay(
    run: number,
    season: number | null,
    episode: number | null,
    watchedAt: string | null | undefined,
  ): Play {
    return {
      id: nextId(),
      run,
      season,
      episode,
      watched_at: watchedAt ?? NOW,
      source: "manual",
      logged_at: NOW,
      voided_at: null,
    };
  }

  for (const [ref, entrySeed] of Object.entries(seed.entries ?? {})) {
    const entry: StoredEntry = {
      status: entrySeed.status ?? "watching",
      score: entrySeed.score ?? null,
      favourite: entrySeed.favourite ?? false,
      notes: entrySeed.notes ?? null,
      runs: entrySeed.runs ?? [{ number: 1, started: null, completed: null }],
      plays: [],
      events: [],
      tags: entrySeed.tags ?? [],
    };
    const run = entry.runs.at(-1)?.number ?? 1;
    for (const w of entrySeed.watched ?? []) {
      const play = newPlay(run, w.season, w.episode, NOW);
      entry.plays.push(play);
      record(entry, { kind: "play_logged", play });
    }
    entries.set(ref, entry);
  }

  function currentRun(entry: StoredEntry): number {
    return entry.runs.at(-1)?.number ?? 1;
  }

  function watched(entry: StoredEntry): WatchedEpisode[] {
    const run = currentRun(entry);
    const seen = new Map<string, WatchedEpisode>();
    for (const p of entry.plays) {
      if (p.run !== run || p.voided_at || p.season == null || p.episode == null)
        continue;
      const key = `${p.season}:${p.episode}`;
      const slot = seen.get(key) ?? {
        season: p.season,
        episode: p.episode,
        play_ids: [],
      };
      slot.play_ids.push(p.id);
      seen.set(key, slot);
    }
    return [...seen.values()].sort(
      (a, b) => a.season - b.season || a.episode - b.episode,
    );
  }

  function summary(ref: string): TitleSummary {
    const title = titles.get(ref);
    if (!title) throw new Error(`unknown title ${ref}`);
    const entry = entries.get(ref);
    return {
      ref: title.ref,
      title: title.title,
      original_title: title.original_title,
      romaji_title: title.romaji_title,
      kind: title.kind,
      category: title.category,
      year: title.year,
      poster_url: title.poster_url,
      genres: title.genres,
      episode_count: title.episode_count,
      library: entry
        ? {
            status: entry.status,
            score: entry.score,
            favourite: entry.favourite,
            rewatching: entry.runs.length > 1,
          }
        : null,
    };
  }

  function entryBody(ref: string, entry: StoredEntry): LibraryEntry {
    const live = entry.plays.filter((p) => !p.voided_at);
    const run = currentRun(entry);
    return {
      title: summary(ref),
      status: entry.status,
      score: entry.score,
      favourite: entry.favourite,
      notes: entry.notes,
      progress: {
        runs: entry.runs.length,
        rewatches: entry.runs.length - 1,
        plays: live.length,
        current_run_plays: live.filter((p) => p.run === run).length,
        current_run_episodes: watched(entry).length,
      },
      tags: entry.tags,
      added_at: NOW,
      updated_at: NOW,
    };
  }

  function detail(ref: string, entry: StoredEntry): LibraryEntryDetail {
    return {
      ...entryBody(ref, entry),
      runs: entry.runs,
      watched: watched(entry),
    };
  }

  function seasonsOf(ref: string) {
    return seed.seasons?.[ref] ?? [];
  }

  function ensureEntry(ref: string): StoredEntry {
    let entry = entries.get(ref);
    if (!entry) {
      entry = {
        status: "watching",
        score: null,
        favourite: false,
        notes: null,
        runs: [{ number: 1, started: null, completed: null }],
        plays: [],
        events: [],
        tags: [],
      };
      entries.set(ref, entry);
    }
    return entry;
  }

  function logPlays(ref: string, body: NewPlays): Play[] {
    const entry = ensureEntry(ref);
    const run = currentRun(entry);
    const have = new Set(watched(entry).map((w) => `${w.season}:${w.episode}`));
    let targets: { season: number | null; episode: number | null }[];
    if (body.kind === "movie") {
      targets = [{ season: null, episode: null }];
    } else if (body.kind === "episodes") {
      targets = body.episodes.filter(
        (e) => !have.has(`${e.season}:${e.episode}`),
      );
    } else {
      targets = seasonsOf(ref)
        .filter(
          ({ season }) => season.number >= 1 && season.number <= body.season,
        )
        .flatMap(({ season, episodes }) =>
          episodes
            .filter(
              (e) => season.number < body.season || e.number <= body.episode,
            )
            .map((e) => ({ season: season.number, episode: e.number })),
        )
        .filter((e) => !have.has(`${e.season}:${e.episode}`));
    }
    const plays = targets.map((t) =>
      newPlay(run, t.season, t.episode, body.watched_at),
    );
    for (const play of plays) {
      entry.plays.push(play);
      record(entry, { kind: "play_logged", play });
    }
    return plays;
  }

  return {
    titles,
    entries,
    record,
    currentRun,
    summary,
    entryBody,
    detail,
    seasonsOf,
    ensureEntry,
    logPlays,
  };
}
