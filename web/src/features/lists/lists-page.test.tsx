// Lists in the real app: the overview and creating a list, a list's
// reorder by drag (driven from the keyboard, as dnd-kit allows) and by the
// move buttons, the rollback of a refused move, and soft deletion.

import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { fakeApi, makeTitle } from "@/test/fake-api";
import {
  A,
  B,
  C,
  item,
  LIST_ID,
  listsServer,
  makeList,
} from "@/test/lists-server";
import { json, renderApp } from "@/test/utils";
import { placementFor } from "./use-lists";

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
