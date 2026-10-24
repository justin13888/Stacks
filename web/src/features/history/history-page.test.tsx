// History in the real app: the feed grouped by day and told in words, undo
// of a play (void it) and of a field change (patch it back), and a title's
// own activity on its page.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { HistoryEvent, Play, TitleSummary } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/features/library/axe";
import { fakeApi, makeTitle } from "@/features/library/fake-api";
import { json, makeSession, mockApi, renderApp } from "@/test/utils";
import { describeChange } from "./describe";

const FRIEREN = makeTitle({
  ref: "anilist:show:154587",
  title: "Frieren",
  category: "anime",
});

const SUMMARY: TitleSummary = { ...FRIEREN, library: null };

function play(id: string, overrides: Partial<Play> = {}): Play {
  return {
    id,
    run: 1,
    season: 1,
    episode: 3,
    watched_at: "2026-10-20T12:00:00Z",
    source: "manual",
    logged_at: "2026-10-20T12:00:00Z",
    voided_at: null,
    ...overrides,
  };
}

const PLAY_ID = "0190a8e0-0000-7000-8000-0000000000p1";
const VOIDED_ID = "0190a8e0-0000-7000-8000-0000000000p2";

const EVENTS: HistoryEvent[] = [
  {
    title: SUMMARY,
    event: {
      id: "e4",
      at: "2026-10-20T15:00:00Z",
      change: {
        kind: "entry_updated",
        before: { status: "watching", score: 80, favourite: false },
        after: { status: "completed", score: 90, favourite: false },
      },
    },
  },
  {
    title: SUMMARY,
    event: {
      id: "e3",
      at: "2026-10-20T13:00:00Z",
      change: { kind: "play_logged", play: play(PLAY_ID) },
    },
  },
  {
    title: SUMMARY,
    event: {
      id: "e2",
      at: "2026-10-18T13:00:00Z",
      change: {
        kind: "play_voided",
        play: play(VOIDED_ID, { episode: 2 }),
      },
    },
  },
  {
    title: SUMMARY,
    event: {
      id: "e1",
      at: "2026-10-18T12:00:00Z",
      change: { kind: "play_logged", play: play(VOIDED_ID, { episode: 2 }) },
    },
  },
];

interface Write {
  method: string;
  path: string;
  body: unknown;
}

/** A history server: the feed, and the writes an undo makes. */
function historyServer(events: HistoryEvent[] = EVENTS) {
  const writes: Write[] = [];
  const api = mockApi(async (request) => {
    const url = new URL(request.url);
    const path = decodeURIComponent(url.pathname).replace(/^\/api\/v1/, "");
    const method = request.method;
    if (method !== "GET") {
      const text = await request.text();
      writes.push({ method, path, body: text ? JSON.parse(text) : undefined });
    }
    if (path === "/sessions/current") return json(makeSession());
    if (method === "GET" && path === "/history") {
      return json({ items: events, next_cursor: null });
    }
    if (method === "DELETE" && path === `/plays/${PLAY_ID}`) {
      return new Response(null, { status: 204 });
    }
    if (method === "PATCH" && path === `/library/${FRIEREN.ref}`) {
      return json({});
    }
    return undefined;
  });
  return {
    api,
    writes,
    writesTo: (prefix: string) =>
      writes.filter((w) => w.path.startsWith(prefix)),
  };
}

describe("describing events", () => {
  test("an episode, a status and a score read as sentences", () => {
    const none = new Set<string>();
    const [updated, logged] = EVENTS.map((e) => e.event.change);
    if (!updated || !logged) throw new Error("fixture events missing");
    expect(describeChange(logged, "Frieren", "point_100", none).lines).toEqual([
      "Watched S1E3 of Frieren",
    ]);
    expect(describeChange(updated, "Frieren", "point_100", none)).toEqual({
      lines: [
        "Changed the status of Frieren: Watching → Completed",
        "Changed the score of Frieren: 80 → 90",
      ],
      undo: {
        kind: "patch-entry",
        changes: { status: "watching", score: 80 },
      },
    });
  });

  test("a movie play names the movie alone", () => {
    expect(
      describeChange(
        {
          kind: "play_logged",
          play: play("m", { season: null, episode: null }),
        },
        "Dune",
        "point_100",
        new Set(),
      ).lines,
    ).toEqual(["Watched Dune"]);
  });
});

describe("history", () => {
  test("the feed is grouped by day, newest first", async () => {
    historyServer();
    const { container } = renderApp("/history");
    expect(
      await screen.findByText("Watched S1E3 of Frieren"),
    ).toBeInTheDocument();
    const days = screen
      .getAllByRole("heading", { level: 2 })
      .map((h) => h.textContent);
    expect(days).toHaveLength(2);
    expect(
      screen.getByText("Undid watching S1E2 of Frieren"),
    ).toBeInTheDocument();
    await expectNoSeriousA11yViolations(container);
  });

  test("undo on a play voids it", async () => {
    const fake = historyServer();
    const user = userEvent.setup();
    renderApp("/history");
    await user.click(
      await screen.findByRole("button", {
        name: "Undo: Watched S1E3 of Frieren",
      }),
    );
    await waitFor(() =>
      expect(fake.writes).toContainEqual({
        method: "DELETE",
        path: `/plays/${PLAY_ID}`,
        body: undefined,
      }),
    );
    expect(await screen.findByText("Undone")).toBeInTheDocument();
  });

  test("undo on a field change patches the entry back", async () => {
    const fake = historyServer();
    const user = userEvent.setup();
    renderApp("/history");
    await user.click(
      await screen.findByRole("button", { name: /^Undo: Changed the status/ }),
    );
    await waitFor(() =>
      expect(fake.writesTo("/library/")).toEqual([
        {
          method: "PATCH",
          path: `/library/${FRIEREN.ref}`,
          body: { status: "watching", score: 80 },
        },
      ]),
    );
  });

  test("a play already voided offers no undo", async () => {
    historyServer();
    renderApp("/history");
    await screen.findByText("Watched S1E2 of Frieren");
    expect(
      screen.queryByRole("button", { name: "Undo: Watched S1E2 of Frieren" }),
    ).not.toBeInTheDocument();
  });

  test("a play restored after its void reads so and can be undone again", async () => {
    const restored = play(VOIDED_ID, { episode: 2 });
    historyServer([
      {
        title: SUMMARY,
        event: {
          id: "e5",
          at: "2026-10-19T09:00:00Z",
          change: { kind: "play_restored", play: restored },
        },
      },
      ...EVENTS,
    ]);
    renderApp("/history");
    expect(
      await screen.findByText("Restored watching S1E2 of Frieren"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Undo: Watched S1E2 of Frieren" }),
    ).toBeInTheDocument();
  });

  test("an empty history says so", async () => {
    historyServer([]);
    renderApp("/history");
    expect(
      await screen.findByRole("heading", { name: "No history yet" }),
    ).toBeInTheDocument();
  });

  test("a title's page shows its own activity, with undo", async () => {
    const fake = fakeApi({
      titles: [FRIEREN],
      entries: { [FRIEREN.ref]: { watched: [{ season: 1, episode: 3 }] } },
    });
    const user = userEvent.setup();
    renderApp(`/titles/${FRIEREN.ref}`);
    const activity = await screen.findByRole("region", { name: "Activity" });
    await user.click(
      await within(activity).findByRole("button", {
        name: "Undo: Watched S1E3 of Frieren",
      }),
    );
    await waitFor(() =>
      expect(fake.writesTo("/plays/").map((w) => w.method)).toEqual(["DELETE"]),
    );
    expect(await screen.findByText("Undone")).toBeInTheDocument();
    expect(fake.entry(FRIEREN.ref)?.watched).toEqual([]);
  });
});
