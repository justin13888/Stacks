// The title page's dated actions in the real app against the stateful fake
// API: marking a movie watched on a past date, starting a rewatch on one,
// and finishing a run on one, each at day, month or year precision; and
// removing the title from the library with an undo.

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import { playTime } from "@/components/ui/when-field";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { fakeApi, makeTitle } from "@/test/fake-api";
import { renderApp } from "@/test/utils";

// Toasts stay until dismissed, so a test can act on one (the undo).
vi.mock("@/components/ui/toast", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/ui/toast")>();
  return {
    ...actual,
    toast: (input: Parameters<typeof actual.toast>[0]) =>
      actual.toast({ ...input, durationMs: Number.POSITIVE_INFINITY }),
  };
});

const MOVIE = makeTitle({
  ref: "tmdb:movie:550",
  title: "Fight Club",
  kind: "movie",
  category: "movie",
});
const SHOW = makeTitle({
  ref: "tmdb:show:1396",
  title: "Breaking Bad",
  kind: "show",
  category: "series",
});

async function openDialog(name: string) {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name }));
  return { user, dialog: await screen.findByRole("dialog") };
}

describe("dated actions on the title page", () => {
  test.each([
    ["planned", { [MOVIE.ref]: { status: "planning" as const } }],
    ["not in the library", {}],
  ])(
    "a %s movie is marked watched in a year: one request dates the play and the run",
    async (_, entries) => {
      const fake = fakeApi({ titles: [MOVIE], entries });
      renderApp(`/titles/${MOVIE.ref}`);

      const { user, dialog } = await openDialog("Mark watched on a date…");
      expect(within(dialog).getByLabelText("Date")).toHaveValue(
        new Date().toLocaleDateString("sv"),
      );
      await user.click(within(dialog).getByRole("radio", { name: "Year" }));
      const year = within(dialog).getByLabelText("Year");
      await user.clear(year);
      await user.type(year, "2019");
      await user.click(
        within(dialog).getByRole("button", { name: /^Mark watched.*2019$/ }),
      );

      await screen.findByText("Watched 2019");
      const year2019 = { date: "2019-01-01", precision: "year" };
      expect(fake.writesTo("/library/")).toEqual([
        {
          method: "POST",
          path: `/library/${MOVIE.ref}/plays`,
          body: {
            kind: "movie",
            watched_at: playTime(year2019 as never),
            watched_on: year2019,
          },
        },
      ]);
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      const history = await screen.findByRole("region", {
        name: "Watch history",
      });
      await within(history).findByText("· started 2019");
      expect(within(history).getByText("· completed 2019")).toBeInTheDocument();
    },
  );

  test("a replay of a finished run keeps the run's dates", async () => {
    const done = { date: "2020-05-01", precision: "day" } as const;
    const fake = fakeApi({
      titles: [MOVIE],
      entries: {
        [MOVIE.ref]: {
          status: "completed",
          runs: [{ number: 1, started: done, completed: done }],
        },
      },
    });
    renderApp(`/titles/${MOVIE.ref}`);

    const { user, dialog } = await openDialog("Mark watched on a date…");
    fireEvent.change(within(dialog).getByLabelText("Date"), {
      target: { value: "2024-05-17" },
    });
    await user.click(within(dialog).getByRole("button", { name: /^Mark/ }));

    await screen.findByText("Marked as watched");
    expect(fake.writesTo("/library/").map((w) => w.method)).toEqual(["POST"]);
  });

  test("a rewatch starts in a month of a year", async () => {
    const fake = fakeApi({
      titles: [SHOW],
      entries: {
        [SHOW.ref]: {
          status: "completed",
          runs: [
            {
              number: 1,
              started: null,
              completed: { date: "2020-01-01", precision: "year" },
            },
          ],
        },
      },
    });
    renderApp(`/titles/${SHOW.ref}`);

    const { user, dialog } = await openDialog("Start a rewatch on a date…");
    await user.click(within(dialog).getByRole("radio", { name: "Month" }));
    await user.click(within(dialog).getByRole("combobox", { name: "Month" }));
    await user.click(await screen.findByRole("option", { name: "March" }));
    const year = within(dialog).getByLabelText("Year");
    await user.clear(year);
    await user.type(year, "2024");
    await user.click(
      within(dialog).getByRole("button", { name: /^Start rewatch/ }),
    );

    await screen.findByText("Rewatch started (run 2)");
    expect(fake.writesTo("/library/")).toEqual([
      {
        method: "POST",
        path: `/library/${SHOW.ref}/runs`,
        body: { started: { date: "2024-03-01", precision: "month" } },
      },
    ]);
    const history = screen.getByRole("region", { name: "Watch history" });
    expect(
      await within(history).findByText(/started Mar 2024/),
    ).toBeInTheDocument();
  });

  test("finishing the current run completes the title on the chosen day", async () => {
    const fake = fakeApi({ titles: [SHOW], entries: { [SHOW.ref]: {} } });
    renderApp(`/titles/${SHOW.ref}`);

    const { user, dialog } = await openDialog("Finish run 1…");
    expect(dialog).toHaveTextContent("The title moves to Completed.");
    fireEvent.change(within(dialog).getByLabelText("Date"), {
      target: { value: "2024-05-17" },
    });
    await user.click(
      within(dialog).getByRole("button", { name: /^Mark finished/ }),
    );

    await screen.findByText("Run 1 finished");
    // One change: the server finishes the run on the date it completes.
    expect(fake.writesTo("/library/")).toEqual([
      {
        method: "PATCH",
        path: `/library/${SHOW.ref}`,
        body: {
          status: "completed",
          on: { date: "2024-05-17", precision: "day" },
        },
      },
    ]);
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "Finish run 1…" }),
      ).not.toBeInTheDocument(),
    );
    expect(fake.entry(SHOW.ref)?.status).toBe("completed");
    expect(fake.entry(SHOW.ref)?.runs[0]?.completed?.date).toBe("2024-05-17");
  });

  test("finishing an older run sets only its date", async () => {
    const started = { date: "2019-02-01", precision: "month" } as const;
    const fake = fakeApi({
      titles: [SHOW],
      entries: {
        [SHOW.ref]: {
          status: "watching",
          runs: [
            { number: 1, started, completed: null },
            { number: 2, started: null, completed: null },
          ],
        },
      },
    });
    renderApp(`/titles/${SHOW.ref}`);

    const { user, dialog } = await openDialog("Finish run 1…");
    expect(dialog).toHaveTextContent("When did you finish this run?");
    fireEvent.change(within(dialog).getByLabelText("Date"), {
      target: { value: "2019-04-02" },
    });
    await user.click(within(dialog).getByRole("button", { name: /^Mark/ }));

    await screen.findByText("Run 1 finished");
    expect(fake.writesTo("/library/")).toEqual([
      {
        method: "PATCH",
        path: `/library/${SHOW.ref}/runs/1`,
        body: { completed: { date: "2019-04-02", precision: "day" } },
      },
    ]);
    expect(fake.entry(SHOW.ref)?.status).toBe("watching");
  });

  const may = { date: "2024-05-20", precision: "day" } as const;
  const finished = { number: 1, started: null, completed: may };
  const rewatch = { number: 2, started: null, completed: null };
  test.each([
    ["Finish run 1…", SHOW, [{ number: 1, started: may, completed: null }]],
    ["Start a rewatch on a date…", SHOW, [finished]],
    ["Mark watched on a date…", MOVIE, [finished, rewatch]],
  ])("%s refuses a date before the bound", async (opener, title, runs) => {
    const fake = fakeApi({
      titles: [title],
      entries: { [title.ref]: { status: "watching", runs } },
    });
    renderApp(`/titles/${title.ref}`);
    const reason = runs[0]?.started
      ? "the run started"
      : "the last run finished";

    const { dialog } = await openDialog(opener);
    fireEvent.change(within(dialog).getByLabelText("Date"), {
      target: { value: "2024-05-17" },
    });

    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      new RegExp(`^That's before ${reason} \\(May 20, 2024\\)\\.$`),
    );
    expect(
      within(dialog).getByRole("button", {
        name: /^(Mark finished|Start rewatch|Mark watched)$/,
      }),
    ).toBeDisabled();
    expect(fake.writesTo("/library/")).toEqual([]);
  });

  test("a date in the future cannot be saved", async () => {
    const fake = fakeApi({ titles: [SHOW], entries: { [SHOW.ref]: {} } });
    renderApp(`/titles/${SHOW.ref}`);

    const { user, dialog } = await openDialog("Finish run 1…");
    await user.click(within(dialog).getByRole("radio", { name: "Year" }));
    const year = within(dialog).getByLabelText("Year");
    await user.clear(year);
    await user.type(year, String(new Date().getFullYear() + 1));

    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "That's in the future.",
    );
    expect(year).toHaveAttribute("aria-invalid", "true");
    expect(
      within(dialog).getByRole("button", { name: /^Mark finished/ }),
    ).toBeDisabled();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(fake.writesTo("/library/")).toEqual([]);
  });

  test("the date dialog has no serious accessibility violations", async () => {
    fakeApi({ titles: [MOVIE], entries: { [MOVIE.ref]: {} } });
    renderApp(`/titles/${MOVIE.ref}`);
    const { user, dialog } = await openDialog("Mark watched on a date…");
    await user.click(within(dialog).getByRole("radio", { name: "Month" }));
    await expectNoSeriousA11yViolations(document.body);
  });
});

describe("removing a title", () => {
  test("moves it to the trash, and the toast's undo restores it", async () => {
    const fake = fakeApi({
      titles: [SHOW],
      entries: { [SHOW.ref]: { score: 80, notes: "Season 4 is the best" } },
    });
    const user = userEvent.setup();
    renderApp(`/titles/${SHOW.ref}`);

    await user.click(
      await screen.findByRole("button", { name: "Remove from library" }),
    );

    await screen.findByText("Removed from your library");
    expect(fake.writesTo("/library/")).toEqual([
      { method: "DELETE", path: `/library/${SHOW.ref}`, body: undefined },
    ]);
    expect(fake.trashed(SHOW.ref)).toBe(true);
    // Back to "not in the library".
    await screen.findByRole("combobox", { name: "Add to library" });

    await user.click(screen.getByRole("button", { name: "Undo" }));
    await screen.findByText("Restored to your library");
    expect(fake.trashed(SHOW.ref)).toBe(false);
    expect(fake.entry(SHOW.ref)?.notes).toBe("Season 4 is the best");
    expect(
      await screen.findByRole("combobox", { name: "Status" }),
    ).toBeInTheDocument();
  });
});
