// Lists in the real app: the overview and creating a list, a list's
// reorder by drag (driven from the keyboard, as dnd-kit allows) and by the
// move buttons, the rollback of a refused move, and soft deletion.

import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import type { List, ListItem, TitleSummary } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { fakeApi, makeTitle } from "@/test/fake-api";
import { json, makeSession, mockApi, problem, renderApp } from "@/test/utils";
import { placementFor } from "./use-lists";

const LIST_ID = "0190a8e0-0000-7000-8000-00000000c0de";

function title(ref: string, name: string): TitleSummary {
  return {
    ref,
    title: name,
    kind: "show",
    category: "anime",
    year: 2023,
    genres: [],
    library: null,
  };
}

const A = title("anilist:show:1", "Alpha");
const B = title("anilist:show:2", "Bravo");
const C = title("anilist:show:3", "Charlie");

function item(t: TitleSummary, position: string): ListItem {
  return {
    title: t,
    position,
    notes: null,
    added_at: "2026-10-01T00:00:00Z",
  };
}

function makeList(overrides: Partial<List> = {}): List {
  return {
    id: LIST_ID,
    name: "Comfort rewatches",
    description: "For bad days",
    smart: false,
    filter: null,
    item_count: 3,
    created_at: "2026-10-01T00:00:00Z",
    updated_at: "2026-10-02T00:00:00Z",
    deleted_at: null,
    ...overrides,
  };
}

interface Write {
  method: string;
  path: string;
  body: unknown;
}

function listsServer(
  options: { lists?: List[]; refuseMoves?: boolean; items?: ListItem[] } = {},
) {
  const writes: Write[] = [];
  const lists = options.lists ?? [makeList()];
  const items = options.items ?? [item(A, "a"), item(B, "b"), item(C, "c")];
  let unreadable = false;
  const api = mockApi(async (request) => {
    const url = new URL(request.url);
    const path = decodeURIComponent(url.pathname).replace(/^\/api\/v1/, "");
    const method = request.method;
    let body: unknown = null;
    if (method !== "GET") {
      const text = await request.text();
      body = text ? JSON.parse(text) : null;
      writes.push({ method, path, body });
    }
    if (path === "/sessions/current") return json(makeSession());
    if (method === "GET" && path === "/lists") {
      return json({ items: lists, next_cursor: null });
    }
    if (method === "POST" && path === "/lists") {
      const created = makeList({
        id: "0190a8e0-0000-7000-8000-00000000beef",
        ...(body as Partial<List>),
        item_count: 0,
      });
      lists.push(created);
      return json(created, 201);
    }
    const one = path.match(/^\/lists\/([^/]+)$/);
    if (one) {
      const list = lists.find((l) => l.id === one[1]);
      if (!list) return problem(404, "list-not-found");
      if (method === "GET") return json(list);
      if (method === "DELETE") return new Response(null, { status: 204 });
      if (method === "PATCH") {
        Object.assign(list, body);
        return json(list);
      }
    }
    if (method === "POST" && path === `/lists/${LIST_ID}/restorations`) {
      return json(lists[0]);
    }
    if (method === "GET" && path === `/lists/${LIST_ID}/items`) {
      if (unreadable) return problem(500, "internal");
      return json({ items, next_cursor: null, degraded: [] });
    }
    if (method === "PATCH" && path.startsWith(`/lists/${LIST_ID}/items/`)) {
      if (options.refuseMoves) {
        // Slow enough to see the move on screen; and from here on the list
        // cannot be read either, so only the rollback can restore the order.
        await new Promise((resolve) => setTimeout(resolve, 100));
        unreadable = true;
        return problem(500, "internal");
      }
      // Applies the move, so the refetch that follows agrees with it.
      const ref = path.slice(`/lists/${LIST_ID}/items/`.length);
      const at = items.findIndex((i) => i.title.ref === ref);
      const [moved] = items.splice(at, 1);
      if (!moved) return problem(404);
      const { after, before } = body as { after?: string; before?: string };
      const to =
        after === null
          ? 0
          : after !== undefined
            ? items.findIndex((i) => i.title.ref === after) + 1
            : before === null
              ? items.length
              : items.findIndex((i) => i.title.ref === before);
      items.splice(to, 0, moved);
      return json(moved);
    }
    return undefined;
  });
  return {
    api,
    writes,
    moves: () =>
      writes.filter((w) => w.method === "PATCH" && w.path.includes("/items/")),
  };
}

/** The list's titles in the order they render. */
function order(): string[] {
  return [...document.querySelectorAll<HTMLElement>("[data-list-item]")].map(
    (row) => row.dataset.listItem ?? "",
  );
}

/** Rows stacked 60px apart, for dnd-kit to measure (jsdom has no layout). */
function layOutRows() {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      const row = this.closest<HTMLElement>("[data-list-item]");
      const rows = [...document.querySelectorAll("[data-list-item]")];
      const index = row ? rows.indexOf(row) : 0;
      return new DOMRect(0, index * 60, 400, 50);
    },
  );
}

describe("placement", () => {
  const items = [item(A, "a"), item(B, "b"), item(C, "c")];
  test("between two neighbours names both", () => {
    expect(placementFor(items, 0, 1, false)).toEqual({
      after: B.ref,
      before: C.ref,
    });
  });
  test("to the start is after null", () => {
    expect(placementFor(items, 2, 0, false)).toEqual({
      after: null,
      before: A.ref,
    });
  });
  test("to the end is before null", () => {
    expect(placementFor(items, 0, 2, false)).toEqual({
      after: C.ref,
      before: null,
    });
  });
  test("last of a partly loaded list names only its upper neighbour", () => {
    expect(placementFor(items, 0, 2, true)).toEqual({ after: C.ref });
  });
});

describe("lists", () => {
  test("the overview lists every list and creates a new one", async () => {
    const server = listsServer();
    const user = userEvent.setup();
    const { container, router } = renderApp("/lists");
    expect(
      await screen.findByRole("link", { name: /Comfort rewatches/ }),
    ).toHaveTextContent("3 titles");
    await expectNoSeriousA11yViolations(container);

    await user.click(screen.getByRole("button", { name: "New list" }));
    const dialog = await screen.findByRole("dialog", { name: "New list" });
    await user.type(within(dialog).getByLabelText("Name"), "Best of 2026");
    await user.click(
      within(dialog).getByRole("button", { name: "Create list" }),
    );
    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "POST",
        path: "/lists",
        body: { name: "Best of 2026", description: null },
      }),
    );
    await waitFor(() =>
      expect(router.state.location.pathname).toBe(
        "/lists/0190a8e0-0000-7000-8000-00000000beef",
      ),
    );
  });

  test("an empty name is refused before anything is sent", async () => {
    const server = listsServer({ lists: [] });
    const user = userEvent.setup();
    renderApp("/lists");
    await screen.findByRole("heading", { name: "No lists yet" });
    await user.click(
      within(screen.getByRole("region", { name: "No lists yet" })).getByRole(
        "button",
        { name: "New list" },
      ),
    );
    const dialog = await screen.findByRole("dialog", { name: "New list" });
    await user.click(
      within(dialog).getByRole("button", { name: "Create list" }),
    );
    expect(
      within(dialog).getByText("Give the list a name."),
    ).toBeInTheDocument();
    expect(server.writes).toEqual([]);
  });
});

describe("a list", () => {
  test("renders its titles in order and passes axe", async () => {
    listsServer();
    const { container } = renderApp(`/lists/${LIST_ID}`);
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Comfort rewatches",
      }),
    ).toBeInTheDocument();
    await screen.findByText("Alpha");
    expect(order()).toEqual([A.ref, B.ref, C.ref]);
    await expectNoSeriousA11yViolations(container);
  });

  test("a keyboard drag sends the drop position as after and before", async () => {
    layOutRows();
    const server = listsServer();
    const user = userEvent.setup();
    renderApp(`/lists/${LIST_ID}`);
    const handle = await screen.findByRole("button", { name: "Drag Alpha" });

    const settle = () => act(() => new Promise((r) => setTimeout(r, 50)));
    handle.focus();
    await user.keyboard(" ");
    await settle();
    await user.keyboard("{ArrowDown}");
    await settle();
    await user.keyboard(" ");

    await waitFor(() =>
      expect(server.moves()).toEqual([
        {
          method: "PATCH",
          path: `/lists/${LIST_ID}/items/${A.ref}`,
          body: { after: B.ref, before: C.ref },
        },
      ]),
    );
  });

  test("the move buttons reorder from the keyboard too", async () => {
    const server = listsServer();
    const user = userEvent.setup();
    renderApp(`/lists/${LIST_ID}`);
    await user.click(
      await screen.findByRole("button", { name: "Move Charlie up" }),
    );
    await waitFor(() => expect(order()).toEqual([A.ref, C.ref, B.ref]));
    await waitFor(() =>
      expect(server.moves()).toEqual([
        {
          method: "PATCH",
          path: `/lists/${LIST_ID}/items/${C.ref}`,
          body: { after: A.ref, before: B.ref },
        },
      ]),
    );
    expect(
      screen.getByRole("button", { name: "Move Alpha up" }),
    ).toBeDisabled();
  });

  test("a refused move puts the title back", async () => {
    listsServer({ refuseMoves: true });
    const user = userEvent.setup();
    renderApp(`/lists/${LIST_ID}`);
    await user.click(
      await screen.findByRole("button", { name: "Move Alpha down" }),
    );
    await waitFor(() => expect(order()).toEqual([B.ref, A.ref, C.ref]));
    await waitFor(() => expect(order()).toEqual([A.ref, B.ref, C.ref]));
    expect((await screen.findAllByText("internal")).length).toBeGreaterThan(0);
  });

  test("deleting moves the list to the trash, with an undo", async () => {
    const server = listsServer();
    const user = userEvent.setup();
    const { router } = renderApp(`/lists/${LIST_ID}`);
    await user.click(await screen.findByRole("button", { name: "Delete" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/lists"));
    await user.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() =>
      expect(server.writes.map((w) => `${w.method} ${w.path}`)).toEqual([
        `DELETE /lists/${LIST_ID}`,
        `POST /lists/${LIST_ID}/restorations`,
      ]),
    );
    await waitFor(() =>
      expect(router.state.location.pathname).toBe(`/lists/${LIST_ID}`),
    );
  });

  test("a smart list shows its titles without reordering", async () => {
    listsServer({
      lists: [makeList({ smart: true, item_count: null })],
      items: [item(A, ""), item(B, "")].map((i) => ({
        ...i,
        position: null,
        added_at: null,
      })),
    });
    renderApp(`/lists/${LIST_ID}`);
    await screen.findByText("Alpha");
    expect(screen.getByText(/A smart list/)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Drag Alpha" }),
    ).not.toBeInTheDocument();
  });
});

describe("adding to a list", () => {
  test("the title page adds its title to a chosen list", async () => {
    const dune = makeTitle({
      ref: "tmdb:movie:438631",
      title: "Dune",
      kind: "movie",
      category: "movie",
    });
    const fake = fakeApi({
      titles: [dune],
      extra: (request, path) => {
        if (request.method === "GET" && path === "/lists") {
          return json({
            items: [
              makeList(),
              makeList({ id: "smart", name: "Smart", smart: true }),
            ],
            next_cursor: null,
          });
        }
        if (request.method === "POST" && path === `/lists/${LIST_ID}/items`) {
          return json(item(A, "z"), 201);
        }
        return undefined;
      },
    });
    const user = userEvent.setup();
    renderApp(`/titles/${dune.ref}`);
    await user.click(
      await screen.findByRole("button", { name: "Add to list" }),
    );
    const dialog = await screen.findByRole("dialog", { name: "Add to a list" });
    expect(
      within(dialog).queryByRole("button", { name: /Smart/ }),
    ).not.toBeInTheDocument();
    await user.click(
      await within(dialog).findByRole("button", { name: /Comfort rewatches/ }),
    );
    await waitFor(() =>
      expect(fake.writesTo("/lists/")).toEqual([
        {
          method: "POST",
          path: `/lists/${LIST_ID}/items`,
          body: { ref: dune.ref },
        },
      ]),
    );
    expect(
      await screen.findByText("Added to Comfort rewatches"),
    ).toBeInTheDocument();
  });
});
