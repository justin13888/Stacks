// Tags in the real app: tagging one title from its page, tagging the
// Library's selection in bulk, the Library's tag filter chips, and the tag
// manager.

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { LibraryEntry, Tag, TitleDetail } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/features/library/axe";
import { fakeApi, makeTitle } from "@/features/library/fake-api";
import { json, renderApp, requestsOf } from "@/test/utils";

const COMFORT: Tag = {
  id: "0190a8e0-0000-7000-8000-0000000000c1",
  name: "Comfort",
  color: "#ff8800",
  entry_count: 2,
};
const SPOOKY: Tag = {
  id: "0190a8e0-0000-7000-8000-0000000000c2",
  name: "Spooky",
  color: null,
  entry_count: 0,
};

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
    added_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-10-01T00:00:00Z",
  };
}

function tagServer() {
  const tags = [COMFORT, SPOOKY];
  return fakeApi({
    titles: [FRIEREN, DUNE],
    entries: { [FRIEREN.ref]: {}, [DUNE.ref]: {} },
    extra: (request, path) => {
      const method = request.method;
      if (method === "GET" && path === "/tags") {
        return json({ items: tags, next_cursor: null });
      }
      if (method === "POST" && path === "/tags") {
        return json(
          { id: "0190a8e0-0000-7000-8000-0000000000c3", entry_count: 0 },
          201,
        );
      }
      if (/^\/tags\/[^/]+$/.test(path)) {
        if (method === "DELETE") return new Response(null, { status: 204 });
        if (method === "PATCH") return json(COMFORT);
      }
      if (/^\/library\/[^/]+\/tags\/[^/]+$/.test(path)) {
        return new Response(null, { status: 204 });
      }
      if (method === "GET" && path === "/library") {
        const tag = new URL(request.url).searchParams.get("tag");
        const items =
          tag === SPOOKY.id ? [] : [entryOf(FRIEREN), entryOf(DUNE)];
        return json({ items, next_cursor: null });
      }
      return undefined;
    },
  });
}

describe("tags", () => {
  test("the title page puts a tag on its title and takes it off", async () => {
    const fake = tagServer();
    const user = userEvent.setup();
    renderApp(`/titles/${FRIEREN.ref}`);
    await user.click(await screen.findByRole("button", { name: "Tags" }));
    const dialog = await screen.findByRole("dialog", { name: "Tags" });
    await user.click(
      await within(dialog).findByRole("button", { name: "Add Comfort" }),
    );
    await waitFor(() =>
      expect(fake.writesTo(`/library/${FRIEREN.ref}/tags`)).toHaveLength(1),
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Remove Comfort" }),
    );
    await waitFor(() =>
      expect(
        fake.writesTo(`/library/${FRIEREN.ref}/tags`).map((w) => w.method),
      ).toEqual(["PUT", "DELETE"]),
    );
    expect(fake.writesTo(`/library/${FRIEREN.ref}/tags`)[0]?.path).toBe(
      `/library/${FRIEREN.ref}/tags/${COMFORT.id}`,
    );
    expect(await screen.findByText("Removed Comfort")).toBeInTheDocument();
  });

  test("a title not in the library offers no tags", async () => {
    const other = makeTitle({ ref: "tmdb:show:1", title: "Other" });
    fakeApi({ titles: [other] });
    renderApp(`/titles/${other.ref}`);
    await screen.findByRole("button", { name: "Add to list" });
    expect(
      screen.queryByRole("button", { name: "Tags" }),
    ).not.toBeInTheDocument();
  });

  test("the Library's list view tags its selection in bulk", async () => {
    const fake = tagServer();
    const user = userEvent.setup();
    const { container } = renderApp("/library?view=list");
    await user.click(
      await screen.findByRole("checkbox", { name: "Select Frieren" }),
    );
    await user.click(screen.getByRole("checkbox", { name: "Select Dune" }));
    const toolbar = screen.getByRole("toolbar", { name: "Selected titles" });
    expect(toolbar).toHaveTextContent("2 titles selected");
    await expectNoSeriousA11yViolations(container);
    await user.click(
      within(toolbar).getByRole("button", { name: "Tag selected" }),
    );
    const dialog = await screen.findByRole("dialog", { name: "Tags" });
    await user.click(
      await within(dialog).findByRole("button", { name: "Add Spooky" }),
    );
    await waitFor(() =>
      expect(
        fake.writes
          .filter((w) => w.path.includes("/tags/"))
          .map((w) => `${w.method} ${w.path}`)
          .sort(),
      ).toEqual([
        `PUT /library/${FRIEREN.ref}/tags/${SPOOKY.id}`,
        `PUT /library/${DUNE.ref}/tags/${SPOOKY.id}`,
      ]),
    );
    expect(await screen.findByText("Tagged Spooky")).toBeInTheDocument();
  });

  test("a tag chip filters the Library, and pressing it again clears it", async () => {
    const fake = tagServer();
    const user = userEvent.setup();
    const { router } = renderApp("/library");
    const chip = await screen.findByRole("button", { name: "Spooky" });
    expect(chip).toHaveAttribute("aria-pressed", "false");
    await user.click(chip);
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ tag: SPOOKY.id }),
    );
    await waitFor(() =>
      expect(
        requestsOf(fake.api).some((r) => r.includes(`tag=${SPOOKY.id}`)),
      ).toBe(true),
    );
    expect(
      await screen.findByRole("heading", {
        name: "Nothing matches these filters",
      }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Spooky" }));
    await waitFor(() =>
      expect(router.state.location.search).not.toHaveProperty("tag"),
    );
  });

  test("the tag manager creates, renames, recolours and deletes tags", async () => {
    const fake = tagServer();
    const user = userEvent.setup();
    renderApp("/library");
    await user.click(
      await screen.findByRole("button", { name: "Manage tags" }),
    );
    const dialog = await screen.findByRole("dialog", { name: "Manage tags" });
    await expectNoSeriousA11yViolations(dialog);

    await user.type(within(dialog).getByLabelText("New tag"), "Cozy");
    await user.click(within(dialog).getByRole("button", { name: "Create" }));
    await waitFor(() =>
      expect(fake.writesTo("/tags")).toContainEqual({
        method: "POST",
        path: "/tags",
        body: { name: "Cozy", color: "#7c5cff" },
      }),
    );

    const name = await within(dialog).findByLabelText("Name of Comfort");
    await user.clear(name);
    await user.type(name, "Comfort food{Enter}");
    await waitFor(() =>
      expect(fake.writesTo(`/tags/${COMFORT.id}`)).toContainEqual({
        method: "PATCH",
        path: `/tags/${COMFORT.id}`,
        body: { name: "Comfort food" },
      }),
    );

    const colour = within(dialog).getByLabelText("Colour of Spooky");
    fireEvent.change(colour, { target: { value: "#112233" } });
    fireEvent.blur(colour);
    await waitFor(() =>
      expect(fake.writesTo(`/tags/${SPOOKY.id}`)).toContainEqual({
        method: "PATCH",
        path: `/tags/${SPOOKY.id}`,
        body: { color: "#112233" },
      }),
    );

    await user.click(
      within(dialog).getByRole("button", { name: "Delete Spooky" }),
    );
    await waitFor(() =>
      expect(fake.writesTo(`/tags/${SPOOKY.id}`)).toContainEqual({
        method: "DELETE",
        path: `/tags/${SPOOKY.id}`,
        body: undefined,
      }),
    );
  });
});
