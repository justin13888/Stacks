// The ⌘K / Ctrl+K palette in the real app, against a mocked API: open,
// search, filter by category, act on a result from the keyboard, navigate.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { TitlePage, TitleSummary } from "@/api/schemas";
import {
  json,
  makeSession,
  mockApi,
  problem,
  renderApp,
  requestsOf,
  route,
} from "@/test/utils";
import { canAddToPlanning, canMarkWatched } from "./command-palette";

function title(
  overrides: Partial<TitleSummary> & Pick<TitleSummary, "ref" | "title">,
): TitleSummary {
  return {
    kind: "movie",
    category: "movie",
    genres: [],
    year: 2021,
    library: null,
    ...overrides,
  };
}

const DUNE = title({ ref: "tmdb:movie:438631", title: "Dune" });
const FRIEREN = title({
  ref: "anilist:show:154587",
  title: "Frieren",
  kind: "show",
  category: "anime",
  year: 2023,
  library: { status: "watching", favourite: false, rewatching: false },
});

function page(
  items: TitleSummary[],
  degraded: TitlePage["degraded"] = [],
): TitlePage {
  return { items, degraded, unreadable: 0 };
}

/** A signed-in app whose search answers by category. */
function searchServer(options: { degraded?: boolean } = {}) {
  const writes: { route: string; body: unknown }[] = [];
  const api = mockApi(async (request) => {
    const url = new URL(request.url);
    switch (route(request)) {
      case "GET /api/v1/sessions/current":
        return json(makeSession());
      case "GET /api/v1/search": {
        const category = url.searchParams.get("category");
        const items = category === "anime" ? [FRIEREN] : [DUNE, FRIEREN];
        return json(
          page(
            items,
            options.degraded
              ? [{ provider: "anilist", class: "unavailable" }]
              : [],
          ),
        );
      }
    }
    if (request.method === "PUT" || request.method === "POST") {
      writes.push({
        route: `${request.method} ${decodeURIComponent(url.pathname)}`,
        body: await request.json(),
      });
      return json({}, 201);
    }
    return problem(404);
  });
  return { api, writes };
}

async function openPalette() {
  await screen.findByTestId("app-shell");
  await userEvent.keyboard("{Control>}k{/Control}");
  return screen.findByRole("dialog", { name: "Search titles" });
}

async function search(text: string) {
  const input = screen.getByRole("combobox", { name: "Search titles" });
  await userEvent.type(input, text);
  await screen.findAllByRole("option", { name: /Dune|Frieren/ });
  return input;
}

describe("command palette", () => {
  test("Ctrl+K opens it with focus in the search field, Escape closes it", async () => {
    searchServer();
    renderApp("/");
    await openPalette();
    expect(
      screen.getByRole("combobox", { name: "Search titles" }),
    ).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("⌘K works too, and the sidebar button opens it", async () => {
    searchServer();
    renderApp("/");
    await screen.findByTestId("app-shell");
    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(
      await screen.findByRole("dialog", { name: "Search titles" }),
    ).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    await userEvent.click(
      screen.getByRole("button", { name: /^Search titles\s*Ctrl K$/ }),
    );
    expect(
      await screen.findByRole("dialog", { name: "Search titles" }),
    ).toBeInTheDocument();
  });

  test("searches as you type (debounced) and lists results as options", async () => {
    const { api } = searchServer();
    renderApp("/");
    await openPalette();
    await search("dune");
    const options = screen.getAllByRole("option");
    expect(options.map((o) => o.getAttribute("aria-label"))).toEqual([
      "Dune, 2021 · Movie",
      "Frieren, 2023 · Anime",
    ]);
    // One request for the settled text, not one per keystroke.
    expect(
      requestsOf(api).filter((r) => r.startsWith("GET /api/v1/search")),
    ).toEqual(["GET /api/v1/search?q=dune"]);
    expect(
      within(options[1] as HTMLElement).getByText("Watching"),
    ).toBeInTheDocument();
  });

  test("a category chip narrows the search", async () => {
    const { api } = searchServer();
    renderApp("/");
    await openPalette();
    await search("fr");
    const anime = screen.getByRole("button", { name: "Anime" });
    await userEvent.click(anime);
    expect(anime).toHaveAttribute("aria-pressed", "true");
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1));
    expect(requestsOf(api)).toContain("GET /api/v1/search?q=fr&category=anime");
  });

  test("Alt+P adds the active title to Planning", async () => {
    const { writes } = searchServer();
    renderApp("/");
    await openPalette();
    await search("dune");
    await userEvent.keyboard("{Alt>}p{/Alt}");
    expect(await screen.findByText("Added to Planning")).toBeInTheDocument();
    expect(writes).toEqual([
      {
        route: "PUT /api/v1/library/tmdb:movie:438631",
        body: { status: "planning" },
      },
    ]);
  });

  test("Alt+W marks the active movie watched", async () => {
    const { writes } = searchServer();
    renderApp("/");
    await openPalette();
    await search("dune");
    await userEvent.keyboard("{Alt>}w{/Alt}");
    expect(await screen.findByText("Marked as watched")).toBeInTheDocument();
    expect(writes).toEqual([
      {
        route: "POST /api/v1/library/tmdb:movie:438631/plays",
        body: { kind: "movie" },
      },
    ]);
  });

  test("actions that could overwrite a tracked title are not offered", async () => {
    const { writes } = searchServer();
    renderApp("/");
    await openPalette();
    await search("fr");
    await userEvent.keyboard("{ArrowDown}{Alt>}p{/Alt}{Alt>}w{/Alt}");
    expect(writes).toEqual([]);
    expect(canAddToPlanning(FRIEREN)).toBe(false);
    expect(canMarkWatched(FRIEREN)).toBe(false);
  });

  test("Enter opens the active title and closes the palette", async () => {
    searchServer();
    const { router } = renderApp("/");
    await openPalette();
    await search("dune");
    await userEvent.keyboard("{ArrowDown}{Enter}");
    await waitFor(() =>
      expect(decodeURIComponent(router.state.location.pathname)).toBe(
        "/titles/anilist:show:154587",
      ),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("a provider outage is reported, and the shell flags the data", async () => {
    searchServer({ degraded: true });
    renderApp("/");
    await openPalette();
    await search("dune");
    expect(screen.getByText(/results may be incomplete/)).toBeInTheDocument();
    expect(
      screen.getByText(/A metadata provider isn't answering/),
    ).toBeInTheDocument();
  });

  test("an upstream failure shows a retryable error", async () => {
    mockApi((request) => {
      switch (route(request)) {
        case "GET /api/v1/sessions/current":
          return json(makeSession());
        case "GET /api/v1/search":
          return problem(503, "upstream-unavailable", { "Retry-After": "0" });
      }
    });
    renderApp("/");
    await openPalette();
    await userEvent.type(
      screen.getByRole("combobox", { name: "Search titles" }),
      "dune",
    );
    const alert = await screen.findByRole("alert", undefined, {
      timeout: 3000,
    });
    expect(alert).toHaveTextContent("A metadata provider is unavailable");
    expect(
      within(alert).getByRole("button", { name: "Try again" }),
    ).toBeInTheDocument();
  });
});
