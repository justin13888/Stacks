// The Library in the real app: the three views, filters in the URL and the
// request, infinite scroll over the cursor, and the status board's drag and
// its keyboard alternatives.

import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { LibraryEntry, Status, TitleDetail } from "@/api/schemas";
import { json, renderApp, requestsOf } from "@/test/utils";
import { expectNoSeriousA11yViolations } from "./axe";
import { fakeApi, makeTitle } from "./fake-api";
import { FILTER_DEBOUNCE_MS } from "./library-page";
import { validateLibrarySearch } from "./search";

function entryOf(title: TitleDetail, status: Status): LibraryEntry {
  return {
    title: { ...title, library: null },
    status,
    score: 70,
    favourite: false,
    notes: null,
    progress: {
      runs: 1,
      rewatches: 0,
      plays: 3,
      current_run_plays: 3,
      current_run_episodes: 3,
    },
    added_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-10-01T00:00:00Z",
  };
}

const FRIEREN = makeTitle({
  ref: "anilist:show:154587",
  title: "Frieren",
  category: "anime",
});
const DUNE = makeTitle({
  ref: "tmdb:movie:438631",
  title: "Dune",
  kind: "movie",
  category: "movie",
});
const CRASH = makeTitle({
  ref: "tmdb:show:94796",
  title: "Crash Landing on You",
  category: "k-drama",
});

const ENTRIES = [
  entryOf(FRIEREN, "watching"),
  entryOf(DUNE, "planning"),
  entryOf(CRASH, "completed"),
];

/** A library server whose listing filters by status and category. */
function libraryServer(
  options: {
    pages?: Record<string, { items: LibraryEntry[]; next?: string }>;
  } = {},
) {
  return fakeApi({
    titles: [FRIEREN, DUNE, CRASH],
    entries: Object.fromEntries(
      ENTRIES.map((e) => [e.title.ref, { status: e.status }]),
    ),
    extra: (request, path) => {
      if (request.method !== "GET" || path !== "/library") return undefined;
      const params = new URL(request.url).searchParams;
      const cursor = params.get("cursor") ?? "first";
      if (options.pages) {
        const page = options.pages[cursor];
        return page
          ? json({ items: page.items, next_cursor: page.next ?? null })
          : undefined;
      }
      const status = params.get("status");
      const category = params.get("category");
      const items = ENTRIES.filter(
        (e) =>
          (!status || e.status === status) &&
          (!category || e.title.category === category),
      );
      return json({ items, next_cursor: null });
    },
  });
}

function libraryRequests(fake: ReturnType<typeof libraryServer>): string[] {
  return requestsOf(fake.api).filter((r) =>
    /^GET \/api\/v1\/library(\?|$)/.test(r),
  );
}

afterEach(() => {
  vi.useRealTimers();
});

describe("library", () => {
  test("the grid shows every entry with its progress and quick +1", async () => {
    libraryServer();
    const { container } = renderApp("/library");
    // The file's first render pays for the cold route imports; under a full
    // parallel run that alone can pass the default 1 s.
    const card = await screen.findByRole(
      "article",
      { name: "Frieren" },
      { timeout: 5000 },
    );
    expect(
      within(card).getByRole("progressbar", { name: /Episodes watched/ }),
    ).toHaveAttribute("aria-valuenow", "3");
    expect(
      within(card).getByRole("button", {
        name: "Log the next episode of Frieren",
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("article", { name: "Dune" })).toBeInTheDocument();
    await expectNoSeriousA11yViolations(container);
  });

  test("the list view is a table whose headers sort the listing", async () => {
    const fake = libraryServer();
    const user = userEvent.setup();
    const { router, container } = renderApp("/library?view=list");
    const table = await screen.findByRole("table");
    await within(table).findByText("Crash Landing on You");
    expect(within(table).getAllByRole("row")).toHaveLength(4);

    await user.click(within(table).getByRole("button", { name: "Title" }));
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({
        view: "list",
        sort: "title",
      }),
    );
    await waitFor(() =>
      expect(libraryRequests(fake).at(-1)).toContain("sort=title"),
    );
    expect(
      within(table).getByRole("columnheader", { name: "Title" }),
    ).toHaveAttribute("aria-sort", "ascending");
    await expectNoSeriousA11yViolations(container);
  });

  test("filters update the URL and the query", async () => {
    const fake = libraryServer();
    const user = userEvent.setup();
    const { router } = renderApp("/library");
    await screen.findByRole("article", { name: "Frieren" });

    await user.click(screen.getByRole("combobox", { name: "Status" }));
    await user.click(await screen.findByRole("option", { name: "Watching" }));
    await waitFor(() =>
      expect(router.state.location.search).toEqual({ status: "watching" }),
    );
    await waitFor(() =>
      expect(libraryRequests(fake).at(-1)).toMatch(/status=watching/),
    );
    await waitFor(() =>
      expect(screen.queryByRole("article", { name: "Dune" })).toBeNull(),
    );

    await user.click(screen.getByRole("combobox", { name: "Category" }));
    await user.click(await screen.findByRole("option", { name: "Anime" }));
    await waitFor(() =>
      expect(router.state.location.search).toEqual({
        status: "watching",
        category: "anime",
      }),
    );
    await waitFor(() =>
      expect(libraryRequests(fake).at(-1)).toMatch(
        /status=watching&category=anime/,
      ),
    );

    await user.type(
      screen.getByRole("searchbox", { name: "Filter by title" }),
      "frie",
    );
    await waitFor(
      () => expect(router.state.location.search).toMatchObject({ q: "frie" }),
      { timeout: FILTER_DEBOUNCE_MS + 1000 },
    );
    await waitFor(() => expect(libraryRequests(fake).at(-1)).toMatch(/q=frie/));
  });

  test("a URL with unknown values is cleaned before it reaches the API", async () => {
    const fake = libraryServer();
    const { router } = renderApp(
      "/library?status=binging&category=anime&view=wall",
    );
    await screen.findByRole("article", { name: "Frieren" });
    expect(router.state.matches.at(-1)?.search).toEqual({ category: "anime" });
    expect(libraryRequests(fake)).toEqual([
      "GET /api/v1/library?category=anime&limit=50",
    ]);
  });

  test("the search validator keeps only values the API accepts", () => {
    expect(
      validateLibrarySearch({
        view: "board",
        status: "paused",
        favourite: "true",
        tag: "not-a-uuid",
        q: "  frieren ",
        sort: "chaos",
      }),
    ).toEqual({
      view: "board",
      status: "paused",
      favourite: true,
      q: "frieren",
    });
  });

  test("infinite scroll requests the next cursor and stops at next_cursor null", async () => {
    let onIntersect: IntersectionObserverCallback | undefined;
    vi.stubGlobal(
      "IntersectionObserver",
      class {
        constructor(callback: IntersectionObserverCallback) {
          onIntersect = callback;
        }
        observe() {}
        disconnect() {}
      },
    );
    const fake = libraryServer({
      pages: {
        first: { items: [ENTRIES[0] as LibraryEntry], next: "page-2" },
        "page-2": { items: [ENTRIES[1] as LibraryEntry] },
      },
    });
    renderApp("/library");
    await screen.findByRole("article", { name: "Frieren" });
    expect(
      screen.getByRole("button", { name: "Load more" }),
    ).toBeInTheDocument();

    const intersect = () =>
      act(() =>
        onIntersect?.(
          [{ isIntersecting: true } as IntersectionObserverEntry],
          {} as IntersectionObserver,
        ),
      );
    intersect();
    await screen.findByRole("article", { name: "Dune" });
    expect(libraryRequests(fake)).toEqual([
      "GET /api/v1/library?limit=50",
      "GET /api/v1/library?cursor=page-2&limit=50",
    ]);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Load more" })).toBeNull(),
    );
    intersect();
    expect(libraryRequests(fake)).toHaveLength(2);
  });

  test("an empty library says how to add something", async () => {
    fakeApi({
      titles: [],
      extra: (_request, path) =>
        path === "/library"
          ? json({ items: [], next_cursor: null })
          : undefined,
    });
    renderApp("/library");
    expect(
      await screen.findByRole("heading", { name: "Your library is empty" }),
    ).toBeInTheDocument();
  });
});

/**
 * jsdom does no layout, so the board's columns are given side-by-side rects
 * (and each card its column's top) for dnd-kit to measure.
 */
function layOutBoard() {
  const columns: Status[] = [
    "planning",
    "watching",
    "completed",
    "paused",
    "dropped",
  ];
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      const column = this.closest<HTMLElement>("[data-board-column]");
      const index = column
        ? columns.indexOf(column.dataset.boardColumn as Status)
        : -1;
      // Outside the columns: the drag overlay, which starts over the card
      // being dragged (in these tests, always from Watching).
      if (index === -1) return new DOMRect(310, 40, 260, 60);
      const left = index * 300;
      if (this === column) return new DOMRect(left, 0, 280, 800);
      return new DOMRect(left + 10, 40, 260, 60);
    },
  );
}

describe("status board", () => {
  test("renders a column per status", async () => {
    libraryServer();
    const { container } = renderApp("/library?view=board");
    for (const name of [
      "Planning",
      "Watching",
      "Completed",
      "Paused",
      "Dropped",
    ]) {
      expect(
        await screen.findByRole("region", { name: new RegExp(`^${name}`) }),
      ).toBeInTheDocument();
    }
    const watching = screen.getByRole("region", { name: /^Watching/ });
    expect(await within(watching).findByText("Frieren")).toBeInTheDocument();
    await expectNoSeriousA11yViolations(container);
  });

  test("dragging a card to another column changes its status", async () => {
    layOutBoard();
    const fake = libraryServer();
    const user = userEvent.setup();
    renderApp("/library?view=board");
    const handle = await screen.findByRole("button", { name: "Drag Frieren" });

    // Pick up, move one column right (Watching → Completed), drop.
    // dnd-kit measures the columns once a drag starts, then follows the keys.
    const settle = () => act(() => new Promise((r) => setTimeout(r, 50)));
    handle.focus();
    await user.keyboard(" ");
    await settle();
    await user.keyboard("{ArrowRight}");
    await settle();
    await user.keyboard(" ");

    await waitFor(() =>
      expect(fake.writesTo("/library/")).toEqual([
        {
          method: "PATCH",
          path: `/library/${FRIEREN.ref}`,
          body: { status: "completed" },
        },
      ]),
    );
    await screen.findByText("Moved to Completed");
  });

  test("the card's Move to menu changes its status from the keyboard", async () => {
    const fake = libraryServer();
    const user = userEvent.setup();
    renderApp("/library?view=board");
    const menu = await screen.findByRole("button", { name: "Move Dune to…" });
    menu.focus();
    await user.keyboard("{Enter}");
    await user.click(await screen.findByRole("menuitem", { name: "Paused" }));
    await waitFor(() =>
      expect(fake.writesTo("/library/")).toEqual([
        {
          method: "PATCH",
          path: `/library/${DUNE.ref}`,
          body: { status: "paused" },
        },
      ]),
    );
  });
});
