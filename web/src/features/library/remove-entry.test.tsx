// Removing a title from each Library view: the grid card's and the list
// row's "more" menu, and the board card's menu. The entry goes to the trash
// and leaves the listing; the toast's undo brings it back.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import type { LibraryEntry, TitleDetail } from "@/api/schemas";
import { fakeApi, makeTitle } from "@/test/fake-api";
import { json, renderApp } from "@/test/utils";

// Toasts stay until dismissed, so a test can act on one (the undo).
vi.mock("@/components/ui/toast", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/ui/toast")>();
  return {
    ...actual,
    toast: (input: Parameters<typeof actual.toast>[0]) =>
      actual.toast({ ...input, durationMs: Number.POSITIVE_INFINITY }),
  };
});

const DUNE = makeTitle({
  ref: "tmdb:movie:438631",
  title: "Dune",
  kind: "movie",
  category: "movie",
});
const FRIEREN = makeTitle({
  ref: "anilist:show:154587",
  title: "Frieren",
  category: "anime",
});

function entryOf(title: TitleDetail): LibraryEntry {
  return {
    title: { ...title, library: null },
    status: "watching",
    score: null,
    favourite: false,
    notes: null,
    progress: {
      runs: 1,
      rewatches: 0,
      plays: 0,
      current_run_plays: 0,
      current_run_episodes: 0,
    },
    tags: [],
    added_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-10-01T00:00:00Z",
  };
}

/** A library whose listing shows the entries still live in the fake. */
function libraryServer() {
  const fake = fakeApi({
    titles: [DUNE, FRIEREN],
    entries: { [DUNE.ref]: {}, [FRIEREN.ref]: {} },
    extra: (request, path) => {
      if (request.method !== "GET" || path !== "/library") return undefined;
      const items = [DUNE, FRIEREN]
        .filter((t) => fake.entry(t.ref))
        .map(entryOf);
      return json({ items, next_cursor: null });
    },
  });
  return fake;
}

describe("removing from the Library", () => {
  test("the grid card's menu removes the title, and undo brings it back", async () => {
    const fake = libraryServer();
    const user = userEvent.setup();
    renderApp("/library");
    const card = await screen.findByRole(
      "article",
      { name: "Dune" },
      { timeout: 5000 },
    );

    await user.click(
      within(card).getByRole("button", { name: "More actions for Dune" }),
    );
    await user.click(
      await screen.findByRole("menuitem", { name: "Remove from library" }),
    );

    await screen.findByText("Removed from your library");
    expect(
      screen.getByText("Dune is in Settings → Trash."),
    ).toBeInTheDocument();
    expect(fake.writesTo("/library/")).toEqual([
      { method: "DELETE", path: `/library/${DUNE.ref}`, body: undefined },
    ]);
    await waitFor(() =>
      expect(
        screen.queryByRole("article", { name: "Dune" }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("article", { name: "Frieren" })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Undo" }));
    await screen.findByText("Restored to your library");
    expect(fake.trashed(DUNE.ref)).toBe(false);
    expect(
      await screen.findByRole("article", { name: "Dune" }),
    ).toBeInTheDocument();
  });

  test("a list row's menu removes the title, from the keyboard", async () => {
    const fake = libraryServer();
    const user = userEvent.setup();
    renderApp("/library?view=list");
    const table = await screen.findByRole("table");
    const menu = await within(table).findByRole("button", {
      name: "More actions for Frieren",
    });
    menu.focus();
    await user.keyboard("{Enter}");
    await screen.findByRole("menuitem", { name: "Remove from library" });
    await user.keyboard("{Enter}");

    await waitFor(() =>
      expect(fake.writesTo("/library/")).toEqual([
        { method: "DELETE", path: `/library/${FRIEREN.ref}`, body: undefined },
      ]),
    );
    expect(fake.trashed(FRIEREN.ref)).toBe(true);
  });

  test("the board card's menu offers removal below its moves", async () => {
    const fake = libraryServer();
    const user = userEvent.setup();
    renderApp("/library?view=board");
    await user.click(
      await screen.findByRole("button", { name: "Move Dune to…" }),
    );
    await user.click(
      await screen.findByRole("menuitem", { name: "Remove from library" }),
    );
    await waitFor(() => expect(fake.trashed(DUNE.ref)).toBe(true));
  });

  test("a refused removal says so and keeps the title", async () => {
    const fake = fakeApi({
      titles: [DUNE],
      entries: { [DUNE.ref]: {} },
      refuse: (method) => method === "DELETE",
      extra: (request, path) =>
        request.method === "GET" && path === "/library"
          ? json({ items: [entryOf(DUNE)], next_cursor: null })
          : undefined,
    });
    const user = userEvent.setup();
    renderApp("/library");
    const card = await screen.findByRole(
      "article",
      { name: "Dune" },
      { timeout: 5000 },
    );
    await user.click(
      within(card).getByRole("button", { name: "More actions for Dune" }),
    );
    await user.click(
      await screen.findByRole("menuitem", { name: "Remove from library" }),
    );
    await waitFor(() => expect(fake.writesTo("/library/")).toHaveLength(1));
    expect(fake.trashed(DUNE.ref)).toBe(false);
    expect(
      screen.queryByText("Removed from your library"),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("article", { name: "Dune" })).toBeInTheDocument();
  });
});
