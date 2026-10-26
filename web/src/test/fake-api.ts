// A stateful fake of the tracking API for component tests: the routes over
// the store in test/fake-store (titles, seasons, library entries with their
// runs, plays and events), behind the mocked `fetch` from test/utils.
// Listings (library, up next, calendar) answer through handlers a test
// supplies. Test-only: no page imports it.

import type {
  EntryChanges,
  LibraryEntryDetail,
  NewPlays,
  Preferences,
  RunDate,
  Tag,
  TitlePage,
  WatchRun,
} from "@/api/schemas";
import { createStore, NOW, type StoreSeed } from "@/test/fake-store";
import { json, makeSession, makeUser, mockApi, problem } from "@/test/utils";

export { makeEpisodes, makeTitle } from "@/test/fake-store";

export interface Write {
  method: string;
  path: string;
  body: unknown;
}

export interface FakeApiOptions extends StoreSeed {
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

export function fakeApi(options: FakeApiOptions) {
  const {
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
  } = createStore(options);
  const writes: Write[] = [];
  // Entries removed from the library, kept whole until restored.
  const trash = new Map<string, NonNullable<ReturnType<typeof entries.get>>>();

  /** Finishes `entry`'s current run on `on`, unless it has finished. */
  function finishOn(entry: { runs: WatchRun[] }, on: RunDate) {
    const run = entry.runs.at(-1);
    if (run && !run.completed) run.completed = on;
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
          if (entry && request.headers.get("if-none-match") === "*") {
            return problem(412, "entry-exists");
          }
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
          const { on, ...changes } = body as EntryChanges;
          for (const [key, value] of Object.entries(changes)) {
            if (value !== undefined) Object.assign(entry, { [key]: value });
          }
          // `completed` on a date finishes the current run then.
          if (on && entry.status === "completed") finishOn(entry, on);
          return json(entryBody(ref, entry));
        }
        if (method === "DELETE") {
          if (!entry) return problem(404, "entry-not-found");
          entries.delete(ref);
          trash.set(ref, entry);
          return new Response(null, { status: 204 });
        }
      }
      if (method === "POST" && rest === "/restorations") {
        const trashed = trash.get(ref);
        if (trashed) {
          trash.delete(ref);
          entries.set(ref, trashed);
          return json(entryBody(ref, trashed));
        }
        return entry
          ? problem(409, "entry-live")
          : problem(404, "trashed-entry-not-found");
      }
      if (method === "POST" && rest === "/runs") {
        if (!entry) return problem(404, "entry-not-found");
        const run: WatchRun = {
          number: currentRun(entry) + 1,
          started: body?.started ?? { date: "2026-10-20", precision: "day" },
          completed: null,
        };
        entry.runs = [...entry.runs, run];
        entry.status = "watching";
        record(entry, { kind: "run_started", run });
        return json(run, 201);
      }
      const runMatch = rest?.match(/^\/runs\/(\d+)$/);
      if (method === "PATCH" && runMatch) {
        const run = entry?.runs.find((r) => r.number === Number(runMatch[1]));
        if (!entry || !run) return problem(404, "run-not-found");
        const before = { ...run };
        Object.assign(run, body);
        record(entry, { kind: "run_updated", before, after: { ...run } });
        return json(run);
      }
      if (method === "POST" && rest === "/plays") {
        const plays = logPlays(ref, body as NewPlays);
        const on = (body as NewPlays).kind === "movie" && body.watched_on;
        const stored = entries.get(ref);
        if (on && stored) {
          // The server's rule: an open run finishes then, starting then if
          // it started later; a finished run keeps its dates.
          const run = stored.runs.at(-1);
          if (
            run &&
            !run.completed &&
            (!run.started || run.started.date > on.date)
          ) {
            run.started = on;
          }
          finishOn(stored, on);
          stored.status = "completed";
        }
        return json({ plays }, 201);
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
    /** Whether `ref`'s entry is in the trash. */
    trashed(ref: string): boolean {
      return trash.has(ref);
    },
  };
}
