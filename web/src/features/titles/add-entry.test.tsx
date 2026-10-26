// The title page's "Add to library" never overwrites an entry already in the
// library: when another tab added the title after the page loaded, the add
// keeps that entry and says so instead of announcing an add.

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { putLibraryEntry } from "@/api/library/library";
import { fakeApi, makeTitle } from "@/test/fake-api";
import { renderApp } from "@/test/utils";

const MOVIE = makeTitle({
  ref: "tmdb:movie:550",
  title: "Fight Club",
  kind: "movie",
  category: "movie",
});

describe("the title page's add", () => {
  test("a stale page keeps the entry another tab added", async () => {
    const fake = fakeApi({ titles: [MOVIE] });
    const user = userEvent.setup();
    renderApp(`/titles/${MOVIE.ref}`);
    const add = await screen.findByRole("combobox", { name: "Add to library" });

    // Another tab adds the title after this page read it as untracked.
    await putLibraryEntry(MOVIE.ref, {
      status: "completed",
      score: 90,
      favourite: true,
      notes: "keep",
    });
    fake.writes.length = 0;

    await user.click(add);
    await user.click(await screen.findByRole("option", { name: "Planning" }));

    expect(
      await screen.findByText("Already in your library"),
    ).toBeInTheDocument();
    expect(screen.queryByText("Added to Planning")).not.toBeInTheDocument();
    expect(
      fake.writesTo("/library/").map((w) => `${w.method} ${w.path}`),
    ).toEqual([`POST /library/${MOVIE.ref}/restorations`]);
    expect(fake.entry(MOVIE.ref)).toMatchObject({
      status: "completed",
      score: 90,
      favourite: true,
      notes: "keep",
    });
    // The page refetches the entry, so it now shows the kept one.
    await waitFor(() =>
      expect(
        screen.queryByRole("combobox", { name: "Add to library" }),
      ).not.toBeInTheDocument(),
    );
  });
});
