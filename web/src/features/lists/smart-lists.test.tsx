// Smart lists in the real app: creating one with a filter, editing its
// filter, the summary of what it picks, and the conversions between the
// form's fields and the API's `ListFilter`.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { A, item, LIST_ID, listsServer, makeList } from "@/test/lists-server";
import { renderApp } from "@/test/utils";
import {
  describeFilter,
  draftFromFilter,
  filterFromDraft,
} from "./smart-filter";

const SMART_FILTER = {
  sort: "trending" as const,
  kinds: ["show" as const],
  genres: ["Drama"],
  year_min: 2010,
  rating_min: 7,
  statuses: ["planning" as const],
};

function smartServer() {
  return listsServer({
    lists: [
      makeList({
        name: "Dramas to try",
        smart: true,
        filter: SMART_FILTER,
        item_count: null,
      }),
    ],
    items: [{ ...item(A, ""), position: null, added_at: null }],
  });
}

describe("the filter's fields", () => {
  test("an empty draft is the popular feed, unnarrowed", () => {
    expect(filterFromDraft(draftFromFilter(null))).toEqual({
      filter: { sort: "popular" },
    });
  });

  test("a stored filter survives the round trip", () => {
    expect(filterFromDraft(draftFromFilter(SMART_FILTER))).toEqual({
      filter: SMART_FILTER,
    });
  });

  test("both kinds are sent as no kind criterion", () => {
    const draft = draftFromFilter({ kinds: ["movie", "show"] });
    expect(draft.kinds).toBe("both");
    expect(filterFromDraft(draft)).toEqual({ filter: { sort: "popular" } });
  });

  test("bounds out of order or out of range are refused", () => {
    const draft = draftFromFilter(null);
    expect(
      filterFromDraft({ ...draft, yearMin: "2020", yearMax: "2010" }),
    ).toEqual({
      errors: { year: "The first year must not be after the second." },
    });
    expect(filterFromDraft({ ...draft, ratingMax: "11" })).toEqual({
      errors: { rating: "Use a whole rating from 0 to 10." },
    });
    expect(filterFromDraft({ ...draft, ratingMin: "6.5" })).toEqual({
      errors: { rating: "Use a whole rating from 0 to 10." },
    });
    expect(filterFromDraft({ ...draft, ratingMin: "-1" })).toEqual({
      errors: { rating: "Use a whole rating from 0 to 10." },
    });
    expect(filterFromDraft({ ...draft, yearMax: "2147483648" })).toEqual({
      errors: { year: "Use a whole year." },
    });
    expect(filterFromDraft({ ...draft, yearMin: "1999.5" })).toEqual({
      errors: { year: "Use a whole year." },
    });
  });

  test("any year the API takes is accepted", () => {
    const draft = draftFromFilter({ year_min: -500, year_max: 3000 });
    expect(filterFromDraft(draft)).toEqual({
      filter: { sort: "popular", year_min: -500, year_max: 3000 },
    });
  });

  test("a genre typed and not yet added is sent", () => {
    const draft = draftFromFilter({ genres: ["Drama"] });
    expect(filterFromDraft({ ...draft, genreText: " Comedy, " })).toEqual({
      filter: { sort: "popular", genres: ["Drama", "Comedy"] },
    });
    expect(filterFromDraft({ ...draft, genreText: "drama" })).toEqual({
      filter: { sort: "popular", genres: ["Drama"] },
    });
  });

  test("the summary names every criterion", () => {
    expect(describeFilter(SMART_FILTER)).toEqual([
      "Trending shows",
      "Drama",
      "Released since 2010",
      "Rated at least 7",
      "In your library as Planning",
    ]);
    expect(describeFilter(null)).toEqual(["Popular movies and shows"]);
  });
});

describe("creating a smart list", () => {
  test("sends the filter the fields describe", async () => {
    const server = listsServer({ lists: [] });
    const user = userEvent.setup();
    const { router } = renderApp("/lists");
    await screen.findByRole("heading", { name: "No lists yet" });
    await user.click(
      screen.getAllByRole("button", { name: "New list" })[0] as HTMLElement,
    );
    const dialog = await screen.findByRole("dialog", { name: "New list" });
    expect(
      within(dialog).queryByRole("region", { name: "Filter" }),
    ).not.toBeInTheDocument();

    await user.click(within(dialog).getByRole("radio", { name: /Smart/ }));
    await user.type(within(dialog).getByLabelText("Name"), "Recent anime");
    const genre = within(dialog).getByLabelText("Add a genre");
    await user.type(genre, "Animation{Enter}Drama,");
    await user.click(
      within(dialog).getByRole("button", { name: "Remove Drama" }),
    );
    await user.type(within(dialog).getByLabelText("From year"), "2015");
    await user.type(within(dialog).getByLabelText("At least"), "8");
    await user.click(within(dialog).getByRole("button", { name: "Watching" }));
    await expectNoSeriousA11yViolations(dialog);
    await user.click(
      within(dialog).getByRole("button", { name: "Create list" }),
    );

    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "POST",
        path: "/lists",
        body: {
          name: "Recent anime",
          description: null,
          filter: {
            sort: "popular",
            genres: ["Animation"],
            year_min: 2015,
            rating_min: 8,
            statuses: ["watching"],
          },
        },
      }),
    );
    await waitFor(() =>
      expect(router.state.location.pathname).toBe(
        "/lists/0190a8e0-0000-7000-8000-00000000beef",
      ),
    );
  });

  test("sends a genre left in the field without Enter", async () => {
    const server = listsServer({ lists: [] });
    const user = userEvent.setup();
    renderApp("/lists");
    await screen.findByRole("heading", { name: "No lists yet" });
    await user.click(
      screen.getAllByRole("button", { name: "New list" })[0] as HTMLElement,
    );
    const dialog = await screen.findByRole("dialog", { name: "New list" });
    await user.click(within(dialog).getByRole("radio", { name: /Smart/ }));
    await user.type(within(dialog).getByLabelText("Name"), "Laughs");
    await user.type(within(dialog).getByLabelText("Add a genre"), "Comedy");
    await user.click(
      within(dialog).getByRole("button", { name: "Create list" }),
    );
    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "POST",
        path: "/lists",
        body: {
          name: "Laughs",
          description: null,
          filter: { sort: "popular", genres: ["Comedy"] },
        },
      }),
    );
  });

  test("an unusable range is refused before anything is sent", async () => {
    const server = listsServer({ lists: [] });
    const user = userEvent.setup();
    renderApp("/lists");
    await screen.findByRole("heading", { name: "No lists yet" });
    await user.click(
      screen.getAllByRole("button", { name: "New list" })[0] as HTMLElement,
    );
    const dialog = await screen.findByRole("dialog", { name: "New list" });
    await user.click(within(dialog).getByRole("radio", { name: /Smart/ }));
    await user.type(within(dialog).getByLabelText("Name"), "Odd");
    await user.type(within(dialog).getByLabelText("From year"), "2020");
    await user.type(within(dialog).getByLabelText("To year"), "2000");
    await user.click(
      within(dialog).getByRole("button", { name: "Create list" }),
    );
    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "The first year must not be after the second.",
    );
    expect(within(dialog).getByLabelText("To year")).toHaveAttribute(
      "aria-invalid",
      "true",
    );
    expect(server.writes).toEqual([]);
  });
});

describe("a smart list", () => {
  test("summarises its filter", async () => {
    smartServer();
    const { container } = renderApp(`/lists/${LIST_ID}`);
    await screen.findByText("Alpha");
    const summary = screen.getByRole("list", { name: "Its filter" });
    expect(
      within(summary)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(describeFilter(SMART_FILTER));
    await expectNoSeriousA11yViolations(container);
  });

  test("editing loads its filter and sends the changed one", async () => {
    const server = smartServer();
    const user = userEvent.setup();
    renderApp(`/lists/${LIST_ID}`);
    await user.click(await screen.findByRole("button", { name: "Edit" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit list" });
    // An existing list keeps its kind.
    expect(within(dialog).queryByRole("radio")).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("From year")).toHaveValue(2010);
    expect(
      within(dialog).getByRole("button", { name: "Planning" }),
    ).toHaveAttribute("aria-pressed", "true");
    await user.click(within(dialog).getByRole("button", { name: "Planning" }));
    await user.clear(within(dialog).getByLabelText("From year"));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "PATCH",
        path: `/lists/${LIST_ID}`,
        body: {
          name: "Dramas to try",
          description: "For bad days",
          filter: {
            sort: "trending",
            kinds: ["show"],
            genres: ["Drama"],
            rating_min: 7,
          },
        },
      }),
    );
  });

  test("a manual list's edit has no filter", async () => {
    const server = listsServer();
    const user = userEvent.setup();
    renderApp(`/lists/${LIST_ID}`);
    await user.click(await screen.findByRole("button", { name: "Edit" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit list" });
    expect(
      within(dialog).queryByRole("region", { name: "Filter" }),
    ).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(server.writes).toContainEqual({
        method: "PATCH",
        path: `/lists/${LIST_ID}`,
        body: { name: "Comfort rewatches", description: "For bad days" },
      }),
    );
  });
});
