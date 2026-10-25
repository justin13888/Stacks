// Up Next (home) in the real app: the API's next episode for each show, a
// one-tap log of exactly that episode, and the Planning movies rail.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { LibraryEntry, UpNextItem } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { fakeApi, makeTitle } from "@/test/fake-api";
import { json, renderApp } from "@/test/utils";

const SHOW = makeTitle({
  ref: "tmdb:show:1396",
  title: "Breaking Bad",
  category: "series",
});
const ANIME = makeTitle({
  ref: "anilist:show:21",
  title: "One Piece",
  category: "anime",
});
const MOVIE = makeTitle({
  ref: "tmdb:movie:550",
  title: "Fight Club",
  kind: "movie",
  category: "movie",
});

const UP_NEXT: UpNextItem[] = [
  {
    title: SHOW,
    season: 2,
    episode: 3,
    name: "Bit by a Dead Bee",
    air_date: "2009-03-22",
    aired: true,
    last_watched_at: "2026-10-19T21:00:00Z",
  },
  {
    title: ANIME,
    season: 1,
    episode: 1100,
    name: null,
    air_date: "2099-01-01",
    aired: false,
  },
];

const PLANNED: LibraryEntry = {
  title: MOVIE,
  status: "planning",
  favourite: false,
  progress: {
    runs: 1,
    rewatches: 0,
    plays: 0,
    current_run_plays: 0,
    current_run_episodes: 0,
  },
  tags: [],
  added_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

function upNextServer(items: UpNextItem[] = UP_NEXT) {
  return fakeApi({
    titles: [SHOW, ANIME, MOVIE],
    entries: {
      [SHOW.ref]: {},
      [ANIME.ref]: {},
      [MOVIE.ref]: { status: "planning" },
    },
    extra: (request, path) => {
      if (path === "/up-next") return json({ items, next_cursor: null });
      if (path === "/library") {
        const params = new URL(request.url).searchParams;
        const planned =
          params.get("status") === "planning" &&
          params.get("category") === "movie";
        return json({ items: planned ? [PLANNED] : [], next_cursor: null });
      }
      return undefined;
    },
  });
}

describe("up next", () => {
  test("shows each show's next episode, newest activity first", async () => {
    upNextServer();
    const { container } = renderApp("/");
    const cards = await screen.findAllByRole("article");
    expect(cards.map((c) => c.getAttribute("aria-label"))).toEqual([
      "Breaking Bad",
      "One Piece",
    ]);
    expect(
      within(cards[0] as HTMLElement).getByText("S2E3"),
    ).toBeInTheDocument();
    expect(
      within(cards[0] as HTMLElement).getByText(/Bit by a Dead Bee/),
    ).toBeInTheDocument();
    expect(
      within(cards[1] as HTMLElement).getByText(/^Airs /),
    ).toBeInTheDocument();
    await expectNoSeriousA11yViolations(container);
  });

  test("one tap logs exactly the episode shown", async () => {
    const fake = upNextServer();
    const user = userEvent.setup();
    renderApp("/");
    await user.click(
      await screen.findByRole("button", { name: "Log S2E3 of Breaking Bad" }),
    );
    await screen.findByText("Logged S2E3");
    expect(fake.writesTo("/library/")).toEqual([
      {
        method: "POST",
        path: `/library/${SHOW.ref}/plays`,
        body: { kind: "episodes", episodes: [{ season: 2, episode: 3 }] },
      },
    ]);
  });

  test("movies on Planning have their own rail", async () => {
    const fake = upNextServer();
    const user = userEvent.setup();
    renderApp("/");
    const rail = await screen.findByRole("region", { name: "Movies to watch" });
    await user.click(
      within(rail).getByRole("button", { name: "Mark Fight Club watched" }),
    );
    await waitFor(() =>
      expect(fake.writesTo("/library/")).toEqual([
        {
          method: "POST",
          path: `/library/${MOVIE.ref}/plays`,
          body: { kind: "movie" },
        },
      ]),
    );
  });

  test("with nothing in progress it says how to start", async () => {
    upNextServer([]);
    renderApp("/");
    expect(
      await screen.findByRole("heading", { name: "Nothing in progress" }),
    ).toBeInTheDocument();
  });

  test("/up-next is the same page", async () => {
    upNextServer();
    const { router } = renderApp("/up-next");
    await screen.findByRole("heading", { level: 1, name: "Up Next" });
    expect(router.state.location.pathname).toBe("/");
  });
});
