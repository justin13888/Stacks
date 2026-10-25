// Discover in the real app: a rail per feed, each failing on its own; cards
// that add to Planning or hide a title; blocked titles never shown; the
// see-all page's infinite scroll; and the blocklist page.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { BlockedTitle, TitlePage, TitleSummary } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { fakeApi, makeTitle } from "@/test/fake-api";
import {
  json,
  makeSession,
  mockApi,
  problem,
  renderApp,
  requestsOf,
} from "@/test/utils";

function summary(ref: string, title: string): TitleSummary {
  return {
    ref,
    title,
    kind: ref.includes(":movie:") ? "movie" : "show",
    category: "movie",
    year: 2024,
    genres: [],
    rating: 7.9,
    library: null,
  };
}

const DUNE = summary("tmdb:movie:693134", "Dune: Part Two");
const ALIEN = summary("tmdb:movie:945961", "Alien: Romulus");
const FRIEREN = {
  ...summary("anilist:show:154587", "Frieren"),
  category: "anime" as const,
};
const QUEEN = {
  ...summary("tmdb:show:226637", "Queen of Tears"),
  category: "k-drama" as const,
};

function page(items: TitleSummary[], next: string | null = null): TitlePage {
  return { items, next_cursor: next, degraded: [], unreadable: 0 };
}

interface Write {
  method: string;
  path: string;
  body: unknown;
}

/** A catalog server: `feeds` by `feed:kind` (kind `-` when absent). */
function discoverServer(
  options: {
    feeds?: Record<string, (cursor: string | null) => Response | undefined>;
    blocked?: BlockedTitle[];
  } = {},
) {
  const writes: Write[] = [];
  const blocked = [...(options.blocked ?? [])];
  const api = mockApi(async (request) => {
    const url = new URL(request.url);
    const path = decodeURIComponent(url.pathname).replace(/^\/api\/v1/, "");
    const method = request.method;
    if (method !== "GET") {
      const text = await request.text();
      writes.push({ method, path, body: text ? JSON.parse(text) : null });
    }
    if (path === "/sessions/current") return json(makeSession());
    const feed = path.match(/^\/feeds\/([^/]+)$/);
    if (method === "GET" && feed) {
      const key = `${feed[1]}:${url.searchParams.get("kind") ?? "-"}`;
      const answer = options.feeds?.[key];
      return answer ? answer(url.searchParams.get("cursor")) : json(page([]));
    }
    if (method === "GET" && path === "/blocklist") {
      return json({ items: blocked, next_cursor: null });
    }
    const block = path.match(/^\/blocklist\/(.+)$/);
    if (block && method === "PUT") return new Response(null, { status: 204 });
    if (block && method === "DELETE") {
      const at = blocked.findIndex((b) => b.title.ref === block[1]);
      if (at >= 0) blocked.splice(at, 1);
      return new Response(null, { status: 204 });
    }
    if (method === "POST" && path.endsWith("/restorations")) {
      return problem(404, "entry-not-found");
    }
    const entry = path.match(/^\/library\/([^/]+)$/);
    if (method === "PUT" && entry) {
      return json({}, 201);
    }
    if (method === "GET" && path === "/lists") {
      return json({ items: [], next_cursor: null });
    }
    return undefined;
  });
  return { api, writes };
}

describe("discover", () => {
  test("a failing rail shows its own error and retry; the others render", async () => {
    let failing = true;
    discoverServer({
      feeds: {
        "trending:movie": () => json(page([DUNE])),
        "airing-anime:-": () =>
          failing
            ? problem(503, "upstream-unavailable", { "retry-after": "0" })
            : json(page([FRIEREN])),
        "k-drama:-": () => json(page([QUEEN])),
      },
    });
    const user = userEvent.setup();
    const { container } = renderApp("/discover");

    const anime = await screen.findByRole("region", { name: "Airing anime" });
    expect(
      await within(anime).findByRole("alert", undefined, { timeout: 3000 }),
    ).toHaveTextContent("A metadata provider is unavailable");
    expect(
      await screen.findByRole("article", { name: "Dune: Part Two" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("article", { name: "Queen of Tears" }),
    ).toBeInTheDocument();
    await expectNoSeriousA11yViolations(container);

    failing = false;
    await user.click(within(anime).getByRole("button", { name: "Try again" }));
    expect(
      await within(anime).findByRole("article", { name: "Frieren" }),
    ).toBeInTheDocument();
  });

  test("blocklisted titles never render in a rail", async () => {
    discoverServer({
      feeds: { "trending:movie": () => json(page([DUNE, ALIEN])) },
      blocked: [{ title: ALIEN, blocked_at: "2026-10-01T00:00:00Z" }],
    });
    renderApp("/discover");
    const rail = await screen.findByRole("region", {
      name: "Trending movies",
    });
    await within(rail).findByRole("article", { name: "Dune: Part Two" });
    await waitFor(() =>
      expect(
        within(rail).queryByRole("article", { name: "Alien: Romulus" }),
      ).not.toBeInTheDocument(),
    );
  });

  test("Not interested hides the card at once and blocks the title", async () => {
    const server = discoverServer({
      feeds: { "trending:movie": () => json(page([DUNE, ALIEN])) },
    });
    const user = userEvent.setup();
    renderApp("/discover");
    const rail = await screen.findByRole("region", {
      name: "Trending movies",
    });
    await within(rail).findByRole("article", { name: "Alien: Romulus" });
    await user.click(
      within(rail).getByRole("button", { name: "More for Alien: Romulus" }),
    );
    await user.click(
      await screen.findByRole("menuitem", { name: "Not interested" }),
    );
    expect(
      within(rail).queryByRole("article", { name: "Alien: Romulus" }),
    ).not.toBeInTheDocument();
    expect(
      within(rail).getByRole("article", { name: "Dune: Part Two" }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "PUT",
        path: `/blocklist/${ALIEN.ref}`,
        body: null,
      }),
    );
    expect(await screen.findByText("Hidden from Discover")).toBeInTheDocument();
  });

  test("a card adds its title to Planning", async () => {
    const server = discoverServer({
      feeds: { "trending:movie": () => json(page([DUNE])) },
    });
    const user = userEvent.setup();
    renderApp("/discover");
    await user.click(
      await screen.findByRole("button", {
        name: "Add Dune: Part Two to Planning",
      }),
    );
    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "PUT",
        path: `/library/${DUNE.ref}`,
        body: { status: "planning" },
      }),
    );
    expect(await screen.findByText("Added to Planning")).toBeInTheDocument();
  });

  test("an in-library title shows its status instead of the add button", async () => {
    discoverServer({
      feeds: {
        "trending:movie": () =>
          json(
            page([
              {
                ...DUNE,
                library: {
                  status: "completed",
                  score: 90,
                  favourite: false,
                  rewatching: false,
                },
              },
            ]),
          ),
      },
    });
    renderApp("/discover");
    const card = await screen.findByRole("article", {
      name: "Dune: Part Two",
    });
    expect(within(card).getByText("Completed")).toBeInTheDocument();
    expect(
      within(card).queryByRole("button", { name: /to Planning/ }),
    ).not.toBeInTheDocument();
  });

  test("see all scrolls a feed page by page", async () => {
    const server = discoverServer({
      feeds: {
        "k-drama:-": (cursor) =>
          cursor === "p2" ? json(page([QUEEN])) : json(page([DUNE], "p2")),
      },
    });
    const user = userEvent.setup();
    const { container } = renderApp("/discover/k-drama");
    expect(
      await screen.findByRole("heading", { level: 1, name: "K-drama" }),
    ).toBeInTheDocument();
    await screen.findByRole("article", { name: "Dune: Part Two" });
    await user.click(screen.getByRole("button", { name: "Load more" }));
    await screen.findByRole("article", { name: "Queen of Tears" });
    expect(requestsOf(server.api)).toContain(
      "GET /api/v1/feeds/k-drama?cursor=p2",
    );
    await expectNoSeriousA11yViolations(container);
  });

  test("an unknown feed is not found", async () => {
    discoverServer();
    renderApp("/discover/nope");
    expect(
      await screen.findByRole("heading", { name: "Nothing here" }),
    ).toBeInTheDocument();
  });
});

describe("recommendations", () => {
  test("blocklisted titles never render among a title's recommendations", async () => {
    const title = makeTitle({ ref: "tmdb:movie:438631", title: "Dune" });
    fakeApi({
      titles: [{ ...title, kind: "movie", category: "movie" }],
      recommendations: { [title.ref]: page([DUNE, ALIEN]) },
      extra: (request, path) => {
        if (request.method === "GET" && path === "/blocklist") {
          return json({
            items: [{ title: ALIEN, blocked_at: "2026-10-01T00:00:00Z" }],
            next_cursor: null,
          });
        }
        return undefined;
      },
    });
    renderApp(`/titles/${title.ref}`);
    const recs = await screen.findByRole("region", { name: "More like this" });
    await within(recs).findByRole("article", { name: "Dune: Part Two" });
    await waitFor(() =>
      expect(
        within(recs).queryByRole("article", { name: "Alien: Romulus" }),
      ).not.toBeInTheDocument(),
    );
  });
});

describe("blocklist page", () => {
  test("lists hidden titles and unblocks one", async () => {
    const server = discoverServer({
      blocked: [{ title: ALIEN, blocked_at: "2026-10-01T00:00:00Z" }],
    });
    const user = userEvent.setup();
    const { container } = renderApp("/blocklist");
    const button = await screen.findByRole("button", {
      name: "Unblock Alien: Romulus",
    });
    await expectNoSeriousA11yViolations(container);
    await user.click(button);
    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "DELETE",
        path: `/blocklist/${ALIEN.ref}`,
        body: null,
      }),
    );
    expect(
      await screen.findByRole("heading", { name: "Nothing hidden" }),
    ).toBeInTheDocument();
  });
});
