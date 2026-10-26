// A card's "add to Planning" never overwrites an entry already in the
// library: a stale card (the feed fetched before the title was added) keeps
// the entry as it is, and every catalog listing is refetched after an add.

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { TitlePage, TitleSummary } from "@/api/schemas";
import { fakeApi, makeTitle } from "@/test/fake-api";
import {
  json,
  makeSession,
  mockApi,
  problem,
  renderApp,
  requestsOf,
} from "@/test/utils";

const DUNE: TitleSummary = {
  ref: "tmdb:movie:693134",
  title: "Dune: Part Two",
  kind: "movie",
  category: "movie",
  year: 2024,
  genres: [],
  rating: 7.9,
  // Stale: the title was added after this listing was fetched.
  library: null,
};

function feed(path: string, url: URL): Response | undefined {
  if (!path.startsWith("/feeds/")) return undefined;
  const trending =
    path === "/feeds/trending" && url.searchParams.get("kind") === "movie";
  const page: TitlePage = {
    items: trending ? [DUNE] : [],
    next_cursor: null,
    degraded: [],
    unreadable: 0,
  };
  return json(page);
}

const ADD = { name: "Add Dune: Part Two to Planning" };

describe("a card's add", () => {
  test("a stale card keeps the entry already in the library", async () => {
    const fake = fakeApi({
      titles: [makeTitle({ ref: DUNE.ref, title: DUNE.title, kind: "movie" })],
      entries: {
        [DUNE.ref]: {
          status: "completed",
          score: 90,
          favourite: true,
          notes: "keep",
        },
      },
      extra: (request, path) => feed(path, new URL(request.url)),
    });
    const user = userEvent.setup();
    renderApp("/discover");
    const feedReads = () =>
      requestsOf(fake.api).filter(
        (r) =>
          r.startsWith("GET /api/v1/feeds/trending?") &&
          r.includes("kind=movie"),
      ).length;

    const button = await screen.findByRole("button", ADD);
    const readsBefore = feedReads();
    await user.click(button);
    expect(
      await screen.findByText("Already in your library"),
    ).toBeInTheDocument();
    expect(screen.queryByText("Added to Planning")).not.toBeInTheDocument();
    expect(
      fake.writesTo("/library/").map((w) => `${w.method} ${w.path}`),
    ).toEqual([`POST /library/${DUNE.ref}/restorations`]);
    expect(fake.entry(DUNE.ref)).toMatchObject({
      status: "completed",
      score: 90,
      favourite: true,
      notes: "keep",
    });
    // The feeds are refetched, so the card shows the entry's status.
    await waitFor(() => expect(feedReads()).toBeGreaterThan(readsBefore));
  });

  test("an add racing another one is create-only and keeps the winner", async () => {
    const puts: (string | null)[] = [];
    mockApi(async (request) => {
      const url = new URL(request.url);
      const path = url.pathname.replace(/^\/api\/v1/, "");
      if (path === "/sessions/current") return json(makeSession());
      if (path.endsWith("/restorations")) {
        return problem(404, "trashed-entry-not-found");
      }
      if (request.method === "PUT" && path.startsWith("/library/")) {
        const condition = request.headers.get("if-none-match");
        puts.push(condition);
        // Another tab added the title since the restore was refused.
        return condition === "*" ? problem(412, "entry-exists") : json({}, 200);
      }
      return feed(path, url);
    });
    const user = userEvent.setup();
    renderApp("/discover");

    await user.click(await screen.findByRole("button", ADD));
    expect(
      await screen.findByText("Already in your library"),
    ).toBeInTheDocument();
    expect(puts).toEqual(["*"]);
  });
});
