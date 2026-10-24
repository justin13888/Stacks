// The title page in the real app against a stateful fake API: logging,
// marking up to an episode, un-watching with undo, rewatching, the entry's
// settings, stale data, failures and the title language.

import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { ScoreFormat } from "@/api/schemas";
import { fakeApi, makeEpisodes, makeTitle } from "@/features/library/fake-api";
import { json, mockApi, problem, renderApp } from "@/test/utils";
import { expectNoSeriousA11yViolations } from "@/features/library/axe";
import { NOTES_AUTOSAVE_MS } from "./library-controls";

const SHOW = makeTitle({
  ref: "tmdb:show:1396",
  title: "Breaking Bad",
  kind: "show",
  category: "series",
  season_count: 2,
  episode_count: 7,
});

function showSeasons() {
  return {
    [SHOW.ref]: [
      {
        season: { number: 0, name: "Specials", episode_count: 2 },
        episodes: makeEpisodes(0, 2),
      },
      {
        season: { number: 1, name: "Season 1", episode_count: 2 },
        episodes: makeEpisodes(1, 2),
      },
      {
        season: { number: 2, name: "Season 2", episode_count: 5 },
        episodes: makeEpisodes(2, 5),
      },
    ],
  };
}

function episodeButton(name: string) {
  return screen.findByRole("button", { name: new RegExp(`^${name}\\b`) });
}

afterEach(() => {
  vi.useRealTimers();
});

describe("title page", () => {
  test("+1 episode logs the next episode, across a season boundary", async () => {
    const fake = fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: {
        [SHOW.ref]: {
          watched: [
            { season: 1, episode: 1 },
            { season: 1, episode: 2 },
          ],
        },
      },
    });
    const user = userEvent.setup();
    renderApp(`/titles/${SHOW.ref}`);

    await user.click(await screen.findByRole("button", { name: "+1 episode" }));

    await screen.findByText("Logged S2E1");
    expect(fake.writesTo("/library/")).toEqual([
      {
        method: "POST",
        path: `/library/${SHOW.ref}/plays`,
        body: { kind: "episodes", episodes: [{ season: 2, episode: 1 }] },
      },
    ]);

    // The next one stays in season 2.
    await user.click(screen.getByRole("button", { name: "+1 episode" }));
    await screen.findByText("Logged S2E2");
  });

  test("mark up to S2E5 issues one bulk play", async () => {
    const fake = fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: { [SHOW.ref]: {} },
    });
    const user = userEvent.setup();
    renderApp(`/titles/${SHOW.ref}`);

    await user.click(await screen.findByRole("tab", { name: "Season 2" }));
    await user.click(
      await screen.findByRole("button", { name: "Mark up to S2E5 watched" }),
    );

    await screen.findByText("Marked up to S2E5 watched");
    expect(fake.writesTo("/library/")).toEqual([
      {
        method: "POST",
        path: `/library/${SHOW.ref}/plays`,
        body: { kind: "through", season: 2, episode: 5 },
      },
    ]);
    await waitFor(() => expect(fake.entry(SHOW.ref)?.watched).toHaveLength(7));
    expect(await episodeButton("S2E5")).toHaveAttribute("aria-pressed", "true");
  });

  test("un-watch voids the play, and the toast's undo logs it again", async () => {
    const fake = fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: { [SHOW.ref]: { watched: [{ season: 1, episode: 1 }] } },
    });
    const user = userEvent.setup();
    renderApp(`/titles/${SHOW.ref}`);

    const tile = await episodeButton("S1E1");
    await waitFor(() => expect(tile).toHaveAttribute("aria-pressed", "true"));
    await user.click(tile);

    const toast = await screen.findByText("Marked S1E1 unwatched");
    await waitFor(() =>
      expect(fake.writes.filter((w) => w.method === "DELETE")).toHaveLength(1),
    );
    expect(fake.writes[0]?.path).toMatch(/^\/plays\//);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /^S1E1\b/ })).toHaveAttribute(
        "aria-pressed",
        "false",
      ),
    );

    const toastRoot = toast.closest("li") ?? document.body;
    await user.click(
      within(toastRoot as HTMLElement).getByRole("button", { name: "Undo" }),
    );

    await waitFor(() =>
      expect(fake.writes.at(-1)).toMatchObject({
        method: "POST",
        path: `/library/${SHOW.ref}/plays`,
        body: { kind: "episodes", episodes: [{ season: 1, episode: 1 }] },
      }),
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /^S1E1\b/ })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    );
  });

  test("a rewatch starts run 2 with no progress, and run 1 stays listed", async () => {
    const fake = fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: {
        [SHOW.ref]: {
          status: "completed",
          runs: [
            {
              number: 1,
              started: { date: "2026-01-02", precision: "day" },
              completed: { date: "2026-02-03", precision: "day" },
            },
          ],
          watched: [
            { season: 1, episode: 1 },
            { season: 1, episode: 2 },
          ],
        },
      },
    });
    const user = userEvent.setup();
    renderApp(`/titles/${SHOW.ref}`);

    await screen.findByText("2 of 7 episodes");
    await user.click(screen.getByRole("button", { name: "Start a rewatch" }));

    await screen.findByText("Rewatch started (run 2)");
    expect(fake.writesTo("/library/")).toEqual([
      { method: "POST", path: `/library/${SHOW.ref}/runs`, body: {} },
    ]);
    await screen.findByText("0 of 7 episodes");
    const history = screen.getByRole("region", { name: "Watch history" });
    expect(within(history).getByText("First watch")).toBeInTheDocument();
    expect(within(history).getByText("Rewatch 1")).toBeInTheDocument();
    expect(within(history).getByText(/completed/)).toBeInTheDocument();
    expect(await episodeButton("S1E1")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  test("specials are shown last and can be logged", async () => {
    const fake = fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: { [SHOW.ref]: {} },
    });
    const user = userEvent.setup();
    renderApp(`/titles/${SHOW.ref}`);

    const tabs = await screen.findAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual([
      "Season 1",
      "Season 2",
      "Specials",
    ]);
    await user.click(screen.getByRole("tab", { name: "Specials" }));
    await user.click(await episodeButton("Special 2"));
    await screen.findByText("Logged Special 2");
    await user.click(
      screen.getByRole("button", { name: "Mark up to Special 2 watched" }),
    );
    expect(fake.writesTo("/library/")[0]?.body).toEqual({
      kind: "episodes",
      episodes: [{ season: 0, episode: 2 }],
    });
    await waitFor(() =>
      expect(fake.writesTo("/library/")[1]?.body).toEqual({
        kind: "episodes",
        episodes: [{ season: 0, episode: 1 }],
      }),
    );
  });

  test("editing notes sends a PATCH with only notes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const fake = fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: { [SHOW.ref]: { notes: "Season 1 was slow", score: 80 } },
    });
    renderApp(`/titles/${SHOW.ref}`);

    const notes = await screen.findByLabelText("Notes");
    fireEvent.change(notes, { target: { value: "Season 2 is great" } });
    await act(() => vi.advanceTimersByTimeAsync(NOTES_AUTOSAVE_MS + 50));

    await waitFor(() =>
      expect(fake.writesTo("/library/")).toEqual([
        {
          method: "PATCH",
          path: `/library/${SHOW.ref}`,
          body: { notes: "Season 2 is great" },
        },
      ]),
    );
    // Leaving the field with nothing new to save sends nothing more.
    fireEvent.blur(notes);
    await screen.findByText("Saved");
    expect(fake.writesTo("/library/")).toHaveLength(1);
  });

  test("a failed notes save is not shown as saved, and leaving the field sends it again", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let refusing = true;
    const fake = fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: { [SHOW.ref]: { notes: "Season 1 was slow" } },
      refuse: (method) => refusing && method === "PATCH",
    });
    renderApp(`/titles/${SHOW.ref}`);

    const notes = await screen.findByLabelText("Notes");
    fireEvent.change(notes, { target: { value: "Season 2 is great" } });
    await act(() => vi.advanceTimersByTimeAsync(NOTES_AUTOSAVE_MS + 50));

    await screen.findByText("Not saved");
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
    expect(notes).toHaveValue("Season 2 is great");
    expect(fake.writesTo("/library/")).toHaveLength(1);

    refusing = false;
    fireEvent.blur(notes);
    await screen.findByText("Saved");
    expect(fake.writesTo("/library/")).toEqual([
      {
        method: "PATCH",
        path: `/library/${SHOW.ref}`,
        body: { notes: "Season 2 is great" },
      },
      {
        method: "PATCH",
        path: `/library/${SHOW.ref}`,
        body: { notes: "Season 2 is great" },
      },
    ]);
    await waitFor(() =>
      expect(fake.entry(SHOW.ref)?.notes).toBe("Season 2 is great"),
    );
  });

  test("status and favourite are PATCHed alone, and a title not tracked can be added", async () => {
    const movie = makeTitle({
      ref: "tmdb:movie:550",
      title: "Fight Club",
      kind: "movie",
      category: "movie",
    });
    const fake = fakeApi({ titles: [movie] });
    const user = userEvent.setup();
    renderApp(`/titles/${movie.ref}`);

    await user.click(
      await screen.findByRole("combobox", { name: "Add to library" }),
    );
    await user.click(await screen.findByRole("option", { name: "Planning" }));
    await screen.findByText("Added to Planning");
    // A trashed entry is restored first; this one isn't in the trash.
    expect(
      fake.writesTo("/library/").map((w) => `${w.method} ${w.path}`),
    ).toEqual([
      `POST /library/${movie.ref}/restorations`,
      `PUT /library/${movie.ref}`,
    ]);

    await user.click(await screen.findByRole("button", { name: "Favourite" }));
    await waitFor(() =>
      expect(fake.writesTo("/library/").at(-1)).toEqual({
        method: "PATCH",
        path: `/library/${movie.ref}`,
        body: { favourite: true },
      }),
    );
    await user.click(screen.getByRole("button", { name: "Mark watched" }));
    await waitFor(() =>
      expect(fake.writesTo("/library/").at(-1)?.body).toEqual({
        kind: "movie",
      }),
    );
  });

  test("a failed write rolls the optimistic change back", async () => {
    fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: { [SHOW.ref]: {} },
      refuse: (method) => method === "PATCH",
      writeLatencyMs: 100,
    });
    const user = userEvent.setup();
    renderApp(`/titles/${SHOW.ref}`);
    const favourite = await screen.findByRole("button", { name: "Favourite" });

    await user.click(favourite);
    // Shown at once, then put back when the server refuses.
    expect(favourite).toHaveAttribute("aria-pressed", "true");
    await screen.findByText("internal");
    await waitFor(() =>
      expect(favourite).toHaveAttribute("aria-pressed", "false"),
    );
  });

  test("a stale title says it is a saved copy", async () => {
    fakeApi({
      titles: [
        {
          ...SHOW,
          freshness: { stale: true, fetched_at: "2026-10-01T00:00:00Z" },
        },
      ],
      seasons: showSeasons(),
    });
    renderApp(`/titles/${SHOW.ref}`);
    expect(
      await screen.findByText(/Saved copy — may be out of date/),
    ).toBeInTheDocument();
  });

  test("an upstream outage shows ErrorState with a retry, not a blank page", async () => {
    let fail = true;
    const api = mockApi((request) => {
      const path = new URL(request.url).pathname.replace(/^\/api\/v1/, "");
      if (path === "/sessions/current") return fakeSession();
      if (path === `/titles/${SHOW.ref}`) {
        return fail ? problem(503, "upstream-unavailable") : json(SHOW);
      }
      return undefined;
    });
    const user = userEvent.setup();
    renderApp(`/titles/${SHOW.ref}`);

    const alert = await screen.findByRole("alert", {}, { timeout: 4000 });
    expect(
      within(alert).getByText("A metadata provider is unavailable"),
    ).toBeInTheDocument();
    fail = false;
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Breaking Bad" }),
    ).toBeInTheDocument();
    expect(api).toHaveBeenCalled();
  });

  test.each([
    ["english", "Frieren: Beyond Journey's End"],
    ["native", "葬送のフリーレン"],
    ["romaji", "Frieren: Beyond Journey's End"],
  ] as const)("the %s title language shows %s", async (language, shown) => {
    const anime = makeTitle({
      ref: "anilist:show:154587",
      title: "Frieren: Beyond Journey's End",
      original_title: "葬送のフリーレン",
      category: "anime",
    });
    fakeApi({ titles: [anime], preferences: { title_language: language } });
    renderApp(`/titles/${anime.ref}`);
    expect(
      await screen.findByRole("heading", { level: 1, name: shown }),
    ).toBeInTheDocument();
  });

  test("recommendations degrade to a note when the provider has none", async () => {
    fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      recommendations: {
        [SHOW.ref]: {
          items: [],
          degraded: [{ provider: "anilist", class: "unsupported" }],
          unreadable: 0,
        },
      },
    });
    renderApp(`/titles/${SHOW.ref}`);
    expect(
      await screen.findByText(/Recommendations aren't available/),
    ).toBeInTheDocument();
  });

  test("the page has no serious accessibility violations", async () => {
    fakeApi({
      titles: [SHOW],
      seasons: showSeasons(),
      entries: { [SHOW.ref]: { watched: [{ season: 1, episode: 1 }] } },
    });
    const { container } = renderApp(`/titles/${SHOW.ref}`);
    await episodeButton("S1E1");
    await expectNoSeriousA11yViolations(container);
  });
});

function fakeSession() {
  return json({
    user: {
      id: "0190a8e0-0000-7000-8000-000000000001",
      display_name: "Ada",
      role: "user",
      created_at: "2026-01-01T00:00:00Z",
      preferences: {
        score_format: "point_10_decimal",
        title_language: "english",
        spoilers: "hide",
        theme: "system",
      },
    },
    session: {
      id: "0190a8e0-0000-7000-8000-0000000000aa",
      current: true,
      created_at: "2026-01-01T00:00:00Z",
      last_seen_at: "2026-01-01T00:00:00Z",
      expires_at: "2026-02-01T00:00:00Z",
    },
  });
}

describe("score input in the title page", () => {
  test.each<[ScoreFormat, number, string, number]>([
    ["point_100", 85, "85", 70],
    ["point_10_decimal", 85, "8.5", 72],
    ["point_10", 80, "8", 60],
  ])(
    "%s shows a stored %i as %s and stores %i",
    async (format, stored, shown, next) => {
      const fake = fakeApi({
        titles: [SHOW],
        seasons: showSeasons(),
        entries: { [SHOW.ref]: { score: stored } },
        preferences: { score_format: format },
      });
      const user = userEvent.setup();
      renderApp(`/titles/${SHOW.ref}`);
      const input = await screen.findByRole("spinbutton", { name: /Score/ });
      expect(input).toHaveValue(Number(shown));
      const typed =
        format === "point_100" ? "70" : format === "point_10" ? "6" : "7.2";
      await user.clear(input);
      await user.type(input, `${typed}{Enter}`);
      await waitFor(() =>
        expect(fake.writesTo("/library/")).toEqual([
          {
            method: "PATCH",
            path: `/library/${SHOW.ref}`,
            body: { score: next },
          },
        ]),
      );
    },
  );

  test.each<[ScoreFormat, number, string, string, number]>([
    ["point_5", 80, "4 stars", "2 stars", 40],
    ["point_3", 100, "Liked", "Disliked", 33],
  ])(
    "%s shows a stored %i as %s and stores %s as %i",
    async (format, stored, checked, pick, next) => {
      const fake = fakeApi({
        titles: [SHOW],
        seasons: showSeasons(),
        entries: { [SHOW.ref]: { score: stored } },
        preferences: { score_format: format },
      });
      const user = userEvent.setup();
      renderApp(`/titles/${SHOW.ref}`);
      expect(await screen.findByRole("radio", { name: checked })).toBeChecked();
      await user.click(screen.getByRole("radio", { name: pick }));
      await waitFor(() =>
        expect(fake.writesTo("/library/")).toEqual([
          {
            method: "PATCH",
            path: `/library/${SHOW.ref}`,
            body: { score: next },
          },
        ]),
      );
      await user.click(screen.getByRole("button", { name: "Clear score" }));
      await waitFor(() =>
        expect(fake.writesTo("/library/").at(-1)?.body).toEqual({
          score: null,
        }),
      );
    },
  );
});
