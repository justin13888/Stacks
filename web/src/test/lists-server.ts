// A fake of the lists API for component tests: the caller's lists, one
// list's items, and every write it was sent. Test-only: no page imports it.

import type { List, ListItem, Tag, TitleSummary } from "@/api/schemas";
import type { Write } from "@/test/fake-api";
import { json, makeSession, mockApi, problem } from "@/test/utils";

export const LIST_ID = "0190a8e0-0000-7000-8000-00000000c0de";

export function title(ref: string, name: string): TitleSummary {
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

export const A = title("anilist:show:1", "Alpha");
export const B = title("anilist:show:2", "Bravo");
export const C = title("anilist:show:3", "Charlie");

export function item(t: TitleSummary, position: string): ListItem {
  return {
    title: t,
    position,
    notes: null,
    added_at: "2026-10-01T00:00:00Z",
  };
}

export function makeList(overrides: Partial<List> = {}): List {
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

export function listsServer(
  options: {
    lists?: List[];
    refuseMoves?: boolean;
    items?: ListItem[];
    tags?: Tag[];
  } = {},
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
    if (method === "GET" && path === "/tags") {
      return json({ items: options.tags ?? [], next_cursor: null });
    }
    if (method === "GET" && path === "/lists") {
      return json({ items: lists, next_cursor: null });
    }
    if (method === "POST" && path === "/lists") {
      const created = makeList({
        id: "0190a8e0-0000-7000-8000-00000000beef",
        ...(body as Partial<List>),
        smart: (body as Partial<List>).filter != null,
        item_count: (body as Partial<List>).filter != null ? null : 0,
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
