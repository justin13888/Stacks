// A stateful fake of the tracking API for component tests: titles, seasons,
// one library entry per title with its runs and plays, and the entry event
// log, behind the mocked `fetch` from test/utils. Listings (library, up
// next, calendar) answer through handlers a test supplies. Test-only: no
// page imports it.

import type {
  Change,
  Episode,
  EntryChanges,
  EntryEvent,
  LibraryEntry,
  LibraryEntryDetail,
  NewPlays,
  Play,
  Preferences,
  Season,
  Status,
  Tag,
  TitleDetail,
  TitlePage,
  TitleSummary,
  WatchedEpisode,
  WatchRun,
} from "@/api/schemas";
import { json, makeSession, makeUser, mockApi, problem } from "@/test/utils";

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

export interface Write {
  method: string;
  path: string;
  body: unknown;
}

export interface FakeApiOptions {
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
  preferences?: Partial<Preferences>;
  recommendations?: Record<string, TitlePage>;
  /**
   * The caller's tags. When given, putting one on an entry or taking it off
   * (`/library/{ref}/tags/{id}`) changes the entry; otherwise those routes
   * fall through to `extra`.
   */
  tags?: Tag[];
  /** Any other route; return `undefined` for a 404. */
  /** How long every write takes to answer, so a test sees it in flight. */
  writeLatencyMs?: number;
  /** Answers a 500 problem to the requests it returns `true` for. */
  refuse?: (method: string, path: string) => boolean;
  extra?: (
    request: Request,
    path: string,
  ) => Response | undefined | Promise<Response | undefined>;
}

let sequence = 0;
function nextId(): string {
  sequence += 1;
  return `0190a8e0-0000-7000-8000-${String(sequence).padStart(12, "0")}`;
}

const NOW = "2026-10-20T12:00:00Z";

export function fakeApi(options: FakeApiOptions) {
  const titles = new Map(options.titles.map((t) => [t.ref, t]));
  const entries = new Map<string, StoredEntry>();
  const writes: Write[] = [];

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

  for (const [ref, seed] of Object.entries(options.entries ?? {})) {
    const entry: StoredEntry = {
      status: seed.status ?? "watching",
      score: seed.score ?? null,
      favourite: seed.favourite ?? false,
      notes: seed.notes ?? null,
      runs: seed.runs ?? [{ number: 1, started: null, completed: null }],
      plays: [],
      events: [],
      tags: seed.tags ?? [],
    };
    const run = entry.runs.at(-1)?.number ?? 1;
    for (const w of seed.watched ?? []) {
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
    return options.seasons?.[ref] ?? [];
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

  const api = mockApi(async (request) => {
    const url = new URL(request.url);
    const path = decodeURIComponent(url.pathname).replace(/^\/api\/v1/, "");
    const method = request.method;
    const body =
      method === "GET" || method === "DELETE"
        ? undefined
        : await request.text().then((text) => (text ? JSON.parse(text) : null));
    if (method !== "GET") writes.push({ method, path, body });
    if (method !== "GET" && options.writeLatencyMs) {
      await new Promise((resolve) =>
        setTimeout(resolve, options.writeLatencyMs),
      );
    }
    if (options.refuse?.(method, path)) return problem(500, "internal");

    if (method === "GET" && path === "/sessions/current") {
      return json(
        makeSession(
          makeUser({
            preferences: {
              ...makeUser().preferences,
              ...options.preferences,
            },
          }),
        ),
      );
    }

    const titleMatch = path.match(/^\/titles\/([^/]+)(\/.*)?$/);
    if (method === "GET" && titleMatch) {
      const [, ref = "", rest] = titleMatch;
      const title = titles.get(ref);
      if (!title) return problem(404, "title-not-found");
      if (!rest) return json({ ...title, library: summary(ref).library });
      if (rest === "/seasons") {
        return json({
          items: seasonsOf(ref).map((s) => s.season),
          freshness: { stale: false },
        });
      }
      const seasonMatch = rest.match(/^\/seasons\/(\d+)$/);
      if (seasonMatch) {
        const number = Number(seasonMatch[1]);
        const found = seasonsOf(ref).find((s) => s.season.number === number);
        if (!found) return problem(404);
        return json({
          number,
          episodes: found.episodes,
          freshness: { stale: false },
        });
      }
      if (rest === "/recommendations") {
        return json(
          options.recommendations?.[ref] ?? {
            items: [],
            degraded: [],
            unreadable: 0,
          },
        );
      }
    }

    const entryMatch = path.match(/^\/library\/([^/]+)(\/.*)?$/);
    if (entryMatch) {
      const [, ref = "", rest] = entryMatch;
      const entry = entries.get(ref);
      if (!rest) {
        if (method === "GET") {
          return entry
            ? json(detail(ref, entry))
            : problem(404, "entry-not-found");
        }
        if (method === "PUT") {
          const created = ensureEntry(ref);
          Object.assign(created, {
            status: body.status,
            score: body.score ?? null,
            favourite: body.favourite ?? false,
            notes: body.notes ?? null,
          });
          return json(entryBody(ref, created), entry ? 200 : 201);
        }
        if (method === "PATCH") {
          if (!entry) return problem(404, "entry-not-found");
          const changes = body as EntryChanges;
          for (const [key, value] of Object.entries(changes)) {
            if (value !== undefined) Object.assign(entry, { [key]: value });
          }
          return json(entryBody(ref, entry));
        }
      }
      if (method === "POST" && rest === "/restorations") {
        return problem(404, "entry-not-found");
      }
      if (method === "POST" && rest === "/runs") {
        if (!entry) return problem(404, "entry-not-found");
        const run: WatchRun = {
          number: currentRun(entry) + 1,
          started: { date: "2026-10-20", precision: "day" },
          completed: null,
        };
        entry.runs = [...entry.runs, run];
        entry.status = "watching";
        record(entry, { kind: "run_started", run });
        return json(run, 201);
      }
      if (method === "POST" && rest === "/plays") {
        return json({ plays: logPlays(ref, body as NewPlays) }, 201);
      }
      const tagMatch = rest?.match(/^\/tags\/([^/]+)$/);
      const tag = options.tags?.find((t) => t.id === tagMatch?.[1]);
      if (
        tagMatch &&
        options.tags &&
        (method === "PUT" || method === "DELETE")
      ) {
        if (!entry) return problem(404, "entry-not-found");
        if (!tag) return problem(404, "tag-not-found");
        entry.tags = entry.tags.filter((t) => t.id !== tag.id);
        if (method === "PUT") {
          entry.tags = [...entry.tags, tag].sort((a, b) =>
            a.name.localeCompare(b.name),
          );
        }
        return new Response(null, { status: 204 });
      }
      if (method === "GET" && rest === "/events") {
        return json({ items: entry?.events ?? [], next_cursor: null });
      }
    }

    const voidMatch = path.match(/^\/plays\/([^/]+)$/);
    if (method === "DELETE" && voidMatch) {
      for (const entry of entries.values()) {
        const play = entry.plays.find((p) => p.id === voidMatch[1]);
        if (play && !play.voided_at) {
          play.voided_at = NOW;
          record(entry, { kind: "play_voided", play: { ...play } });
          return new Response(null, { status: 204 });
        }
      }
      return problem(404, "play-not-found");
    }

    const restoreMatch = path.match(/^\/plays\/([^/]+)\/restorations$/);
    if (method === "POST" && restoreMatch) {
      for (const entry of entries.values()) {
        const play = entry.plays.find((p) => p.id === restoreMatch[1]);
        if (!play) continue;
        if (!play.voided_at) return problem(409, "play-not-voided");
        play.voided_at = null;
        record(entry, { kind: "play_restored", play: { ...play } });
        return json({ ...play });
      }
      return problem(404, "play-not-found");
    }

    return options.extra?.(request, path);
  });

  return {
    api,
    writes,
    /** The writes to `path`, in order. */
    writesTo(pathPrefix: string): Write[] {
      return writes.filter((w) => w.path.startsWith(pathPrefix));
    },
    entry(ref: string): LibraryEntryDetail | undefined {
      const entry = entries.get(ref);
      return entry ? detail(ref, entry) : undefined;
    },
  };
}
