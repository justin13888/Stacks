// The trash in the real app: deleted library entries and lists, each
// restored to where it came from.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { LibraryEntry, List } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { makeTitle } from "@/test/fake-api";
import {
  json,
  makeSession,
  mockApi,
  renderApp,
  requestsOf,
  route,
} from "@/test/utils";

const DUNE = makeTitle({
  ref: "tmdb:movie:438631",
  title: "Dune",
  kind: "movie",
  category: "movie",
});

const DELETED_AT = "2026-10-18T09:30:00Z";

const ENTRY: LibraryEntry = {
  title: { ...DUNE, library: null },
  status: "completed",
  score: 90,
  favourite: true,
  notes: "Rewatch in IMAX",
  tags: [],
  progress: {
    runs: 1,
    rewatches: 0,
    plays: 1,
    current_run_plays: 1,
    current_run_episodes: 0,
  },
  added_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-10-18T09:30:00Z",
  deleted_at: DELETED_AT,
};

const LIST: List = {
  id: "0190a8e0-0000-7000-8000-0000000000b1",
  name: "Sci-fi night",
  smart: false,
  item_count: 3,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-10-18T09:30:00Z",
  deleted_at: DELETED_AT,
};

function trashServer() {
  let entry = ENTRY;
  let list = LIST;
  const api = mockApi((request) => {
    const url = new URL(request.url);
    const deleted = url.searchParams.get("deleted") === "true";
    switch (route(request)) {
      case "GET /api/v1/sessions/current":
        return json(makeSession());
      case "GET /api/v1/library":
        return json({
          items: Boolean(entry.deleted_at) === deleted ? [entry] : [],
          next_cursor: null,
        });
      case `POST /api/v1/library/${encodeURIComponent(DUNE.ref)}/restorations`:
      case `POST /api/v1/library/${DUNE.ref}/restorations`:
        entry = { ...entry, deleted_at: null };
        return json(entry);
      case "GET /api/v1/lists":
        return json({
          items: Boolean(list.deleted_at) === deleted ? [list] : [],
          next_cursor: null,
        });
      case `POST /api/v1/lists/${LIST.id}/restorations`:
        list = { ...list, deleted_at: null };
        return json(list);
    }
    return undefined;
  });
  return api;
}

describe("trash", () => {
  test("a restored title leaves the trash and is back in the Library", async () => {
    const api = trashServer();
    const { router } = renderApp("/settings/trash");
    const restore = await screen.findByRole(
      "button",
      { name: "Restore Dune" },
      { timeout: 5000 },
    );
    // Each row says when it was deleted.
    expect(restore.closest("li")).toHaveTextContent(/deleted \S/);
    await userEvent.click(restore);

    await screen.findByText("No deleted titles");
    expect(
      requestsOf(api)
        .filter((r) => r.startsWith("POST"))
        .map(decodeURIComponent),
    ).toEqual([`POST /api/v1/library/${DUNE.ref}/restorations`]);

    await router.navigate({ to: "/library" });
    expect(
      await screen.findByRole("article", { name: "Dune" }, { timeout: 5000 }),
    ).toBeInTheDocument();
  });

  test("a restored list leaves the trash and is back in Lists", async () => {
    const api = trashServer();
    const { router } = renderApp("/settings/trash");
    await userEvent.click(
      await screen.findByRole(
        "button",
        { name: "Restore Sci-fi night" },
        { timeout: 5000 },
      ),
    );
    await screen.findByText("No deleted lists");
    expect(requestsOf(api)).toContain(
      `POST /api/v1/lists/${LIST.id}/restorations`,
    );

    await router.navigate({ to: "/lists" });
    const main = await screen.findByRole("main");
    await waitFor(() =>
      expect(within(main).getByText("Sci-fi night")).toBeInTheDocument(),
    );
  });

  test("the page has no serious accessibility violations", async () => {
    trashServer();
    const { container } = renderApp("/settings/trash");
    await screen.findByRole(
      "button",
      { name: "Restore Dune" },
      { timeout: 5000 },
    );
    await expectNoSeriousA11yViolations(container);
  });
});
