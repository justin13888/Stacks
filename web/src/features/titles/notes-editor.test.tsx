// The notes editor sends one save at a time: alone, with each save settled by
// the test, and in the title page against the stateful fake API.

import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { fakeApi, makeEpisodes, makeTitle } from "@/test/fake-api";
import { renderApp } from "@/test/utils";
import { NotesEditor } from "./library-controls";

const SHOW = makeTitle({
  ref: "tmdb:show:1396",
  title: "Breaking Bad",
  kind: "show",
  category: "series",
  season_count: 1,
  episode_count: 2,
});

/**
 * Holds every PATCH the fake API receives until the test delivers it, so the
 * test chooses the order the writes reach the server.
 */
function holdPatches(fake: ReturnType<typeof fakeApi>) {
  const held: (() => void)[] = [];
  const reach = fake.api.getMockImplementation();
  if (!reach) throw new Error("the fake API has no implementation");
  fake.api.mockImplementation(async (input, init) => {
    if (init?.method === "PATCH") {
      await new Promise<void>((resolve) => held.push(resolve));
    }
    return reach(input, init);
  });
  return {
    /** How many held writes have not reached the server yet. */
    count: () => held.length,
    /** Lets the most recent held write reach the server. */
    deliverLatest: () => held.pop()?.(),
  };
}

describe("notes editor in the title page", () => {
  test("notes saves reach the server in order, so an older note never wins", async () => {
    const fake = fakeApi({
      titles: [SHOW],
      seasons: {
        [SHOW.ref]: [
          {
            season: { number: 1, name: "Season 1", episode_count: 2 },
            episodes: makeEpisodes(1, 2),
          },
        ],
      },
      entries: { [SHOW.ref]: { notes: "Season 1 was slow" } },
    });
    const held = holdPatches(fake);
    renderApp(`/titles/${SHOW.ref}`);

    const notes = await screen.findByLabelText("Notes");
    fireEvent.change(notes, { target: { value: "A" } });
    fireEvent.blur(notes);
    await waitFor(() => expect(held.count()).toBe(1));
    fireEvent.change(notes, { target: { value: "B" } });
    fireEvent.blur(notes);
    // Give B every chance to be sent alongside A: it must wait instead.
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
    expect(held.count()).toBe(1);

    // Deliver the newest write first each time: a B already sent would
    // overtake A here and leave A stored.
    await act(async () => held.deliverLatest());
    await waitFor(() => expect(held.count()).toBe(1));
    await act(async () => held.deliverLatest());

    await screen.findByText("Saved");
    expect(fake.entry(SHOW.ref)?.notes).toBe("B");
    expect(fake.writesTo("/library/")).toEqual([
      { method: "PATCH", path: `/library/${SHOW.ref}`, body: { notes: "A" } },
      { method: "PATCH", path: `/library/${SHOW.ref}`, body: { notes: "B" } },
    ]);
  });
});

describe("notes editor saving one at a time", () => {
  // "A" is sent, then "B" is typed and the field left before A settles, so
  // B waits for A. Each save's promise is settled by the test.
  function renderQueuedSave() {
    const settles: Record<
      string,
      { resolve: () => void; reject: (error: Error) => void }
    > = {};
    const onSave = vi.fn(
      (value: string) =>
        new Promise<void>((resolve, reject) => {
          settles[value] = { resolve, reject };
        }),
    );
    render(<NotesEditor notes="" onSave={onSave} />);
    const notes = screen.getByLabelText("Notes");
    fireEvent.change(notes, { target: { value: "A" } });
    fireEvent.blur(notes);
    fireEvent.change(notes, { target: { value: "B" } });
    fireEvent.blur(notes);
    expect(onSave.mock.calls).toEqual([["A"]]);
    expect(screen.getByText("Saving…")).toBeInTheDocument();
    return { notes, onSave, settles };
  }

  test("the earlier save succeeding sends the later, which failing shows not saved", async () => {
    const { notes, onSave, settles } = renderQueuedSave();

    await act(async () => settles.A?.resolve());
    expect(onSave.mock.calls).toEqual([["A"], ["B"]]);
    expect(screen.getByText("Saving…")).toBeInTheDocument();
    await act(async () => settles.B?.reject(new Error("refused")));

    expect(screen.getByText("Not saved")).toBeInTheDocument();
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
    // B is still unsaved, so leaving the field sends it again.
    fireEvent.blur(notes);
    expect(onSave.mock.calls.at(-1)).toEqual(["B"]);
  });

  test("the earlier save failing still sends the later, which succeeding shows saved", async () => {
    const { notes, onSave, settles } = renderQueuedSave();

    await act(async () => settles.A?.reject(new Error("refused")));
    expect(onSave.mock.calls).toEqual([["A"], ["B"]]);
    expect(screen.getByText("Saving…")).toBeInTheDocument();
    await act(async () => settles.B?.resolve());

    expect(screen.getByText("Saved")).toBeInTheDocument();
    // B was accepted, so there is nothing left to send.
    fireEvent.blur(notes);
    expect(onSave).toHaveBeenCalledTimes(2);
  });

  test("both saves failing shows not saved, and neither text counts as stored", async () => {
    const { notes, onSave, settles } = renderQueuedSave();

    await act(async () => settles.A?.reject(new Error("refused")));
    await act(async () => settles.B?.reject(new Error("refused")));

    expect(screen.getByText("Not saved")).toBeInTheDocument();
    // Neither save was stored, so typing "A" again sends it.
    fireEvent.change(notes, { target: { value: "A" } });
    fireEvent.blur(notes);
    expect(onSave.mock.calls.at(-1)).toEqual(["A"]);
    expect(onSave).toHaveBeenCalledTimes(3);
  });

  test("changes made while a save is in flight send only the latest after it", async () => {
    const { notes, onSave, settles } = renderQueuedSave();

    fireEvent.change(notes, { target: { value: "C" } });
    fireEvent.blur(notes);
    expect(onSave).toHaveBeenCalledTimes(1);
    await act(async () => settles.A?.resolve());
    expect(onSave.mock.calls).toEqual([["A"], ["C"]]);
    await act(async () => settles.C?.resolve());

    expect(screen.getByText("Saved")).toBeInTheDocument();
  });

  test("going back to the text in flight sends nothing after it", async () => {
    const { notes, onSave, settles } = renderQueuedSave();

    fireEvent.change(notes, { target: { value: "A" } });
    fireEvent.blur(notes);
    await act(async () => settles.A?.resolve());

    expect(onSave.mock.calls).toEqual([["A"]]);
    expect(screen.getByText("Saved")).toBeInTheDocument();
  });

  test("going back to the stored text while a save fails shows saved", async () => {
    const { notes, onSave, settles } = renderQueuedSave();

    fireEvent.change(notes, { target: { value: "" } });
    fireEvent.blur(notes);
    await act(async () => settles.A?.reject(new Error("refused")));

    // The server still holds the loaded "", which is what the field shows.
    expect(onSave.mock.calls).toEqual([["A"]]);
    expect(screen.getByText("Saved")).toBeInTheDocument();
  });
});
