// Smart lists that draw on the user's library: categories, tags and the
// user's own score, in the fields and in the real app.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { ListFilter, Tag } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { listsServer } from "@/test/lists-server";
import { renderApp } from "@/test/utils";
import {
  describeFilter,
  draftFromFilter,
  filterFromDraft,
} from "./smart-filter";

const COMFORT: Tag = {
  id: "0190a8e0-0000-7000-8000-0000000000c1",
  name: "comfort",
  color: "#aabbcc",
  entry_count: 2,
};
const GONE = "0190a8e0-0000-7000-8000-0000000000c2";

const LIBRARY_FILTER: ListFilter = {
  sort: "library",
  categories: ["k-drama", "anime"],
  tags: [COMFORT.id, GONE],
  score_min: 85,
  score_max: 100,
};

describe("the library criteria's fields", () => {
  test("a stored filter survives the round trip in a coarser format", () => {
    const draft = draftFromFilter(LIBRARY_FILTER, "point_5");
    expect(draft.scoreMin).toBe("4");
    expect(filterFromDraft(draft)).toEqual({ filter: LIBRARY_FILTER });
  });

  test("a score is entered in the user's format and sent as 0–100", () => {
    const decimal = draftFromFilter(null, "point_10_decimal");
    expect(filterFromDraft({ ...decimal, scoreMin: "8.5" })).toEqual({
      filter: { sort: "popular", score_min: 85 },
    });
    const stars = draftFromFilter(LIBRARY_FILTER, "point_5");
    // 50 is the lowest stored score shown as 3 / 5.
    expect(filterFromDraft({ ...stars, scoreMin: "3" })).toEqual({
      filter: { ...LIBRARY_FILTER, score_min: 50 },
    });
  });

  test("a typed bound keeps every score shown as that value", () => {
    const tens = draftFromFilter(null, "point_10");
    // 85 to 94 are all shown as 9 / 10.
    expect(filterFromDraft({ ...tens, scoreMin: "9", scoreMax: "9" })).toEqual({
      filter: { sort: "popular", score_min: 85, score_max: 94 },
    });
    expect(
      describeFilter({ score_min: 85 }, { scoreFormat: "point_10" }),
    ).toContain("You scored at least 9 / 10");
    const thirds = draftFromFilter(null, "point_3");
    // Liked is 67 and above.
    expect(filterFromDraft({ ...thirds, scoreMin: "3" })).toEqual({
      filter: { sort: "popular", score_min: 67 },
    });
  });

  test("a score outside the format or out of order is refused", () => {
    const draft = draftFromFilter(null, "point_10");
    const refused = (scoreMin: string, scoreMax = "") =>
      filterFromDraft({ ...draft, scoreMin, scoreMax });
    expect(refused("11")).toEqual({
      errors: { score: "Use a score from 0 to 10." },
    });
    expect(refused("8.5")).toEqual({
      errors: { score: "Use a score from 0 to 10." },
    });
    expect(refused("8", "6")).toEqual({
      errors: { score: "The first score must not be after the second." },
    });
  });

  test("the summary names the source, categories, tags and score", () => {
    const tagNames = new Map([[COMFORT.id, COMFORT.name]]);
    expect(
      describeFilter(LIBRARY_FILTER, {
        scoreFormat: "point_10_decimal",
        tagNames,
      }),
    ).toEqual([
      "Movies and shows from your library",
      "K-drama or Anime",
      "Tagged comfort or a deleted tag",
      "You scored 8.5–10.0",
    ]);
  });
});

describe("creating a library smart list", () => {
  test("sends the categories, tags and score chosen", async () => {
    const server = listsServer({ lists: [], tags: [COMFORT] });
    const user = userEvent.setup();
    renderApp("/lists");
    await screen.findByRole("heading", { name: "No lists yet" });
    await user.click(
      screen.getAllByRole("button", { name: "New list" })[0] as HTMLElement,
    );
    const dialog = await screen.findByRole("dialog", { name: "New list" });
    await user.click(within(dialog).getByRole("radio", { name: /Smart/ }));
    await user.type(within(dialog).getByLabelText("Name"), "Loved K-dramas");
    await user.click(
      within(dialog).getByRole("combobox", { name: "Draw from" }),
    );
    await user.click(
      await screen.findByRole("option", { name: "Your library" }),
    );
    await user.click(within(dialog).getByRole("button", { name: "K-drama" }));
    await user.click(
      await within(dialog).findByRole("button", { name: "comfort" }),
    );
    await user.type(within(dialog).getByLabelText("Scored at least"), "8.5");
    await expectNoSeriousA11yViolations(dialog);
    await user.click(
      within(dialog).getByRole("button", { name: "Create list" }),
    );

    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "POST",
        path: "/lists",
        body: {
          name: "Loved K-dramas",
          description: null,
          filter: {
            sort: "library",
            categories: ["k-drama"],
            tags: [COMFORT.id],
            score_min: 85,
          },
        },
      }),
    );
  });
});
