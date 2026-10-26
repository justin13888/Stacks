// The notes editor on its own, with two saves in flight at once: whichever
// order they settle in, it shows the state of the latest text.

import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { NotesEditor } from "./library-controls";

describe("notes editor with overlapping saves", () => {
  // Two saves in flight at once: "A" sent, then "B" sent before A settles.
  // Each save's promise is settled by the test, in the order it chooses.
  function renderOverlappingSaves() {
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
    expect(onSave.mock.calls).toEqual([["A"], ["B"]]);
    expect(screen.getByText("Saving…")).toBeInTheDocument();
    return { notes, onSave, settles };
  }

  test("the earlier save succeeding, then the later failing, shows not saved", async () => {
    const { notes, onSave, settles } = renderOverlappingSaves();

    await act(async () => settles.A?.resolve());
    expect(screen.getByText("Saving…")).toBeInTheDocument();
    await act(async () => settles.B?.reject(new Error("refused")));

    expect(screen.getByText("Not saved")).toBeInTheDocument();
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
    // B is still unsaved, so leaving the field sends it again.
    fireEvent.blur(notes);
    expect(onSave.mock.calls.at(-1)).toEqual(["B"]);
  });

  test("the later save failing, then the earlier succeeding, stays not saved", async () => {
    const { notes, onSave, settles } = renderOverlappingSaves();

    await act(async () => settles.B?.reject(new Error("refused")));
    expect(screen.getByText("Not saved")).toBeInTheDocument();
    await act(async () => settles.A?.resolve());

    expect(screen.getByText("Not saved")).toBeInTheDocument();
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
    fireEvent.blur(notes);
    expect(onSave.mock.calls.at(-1)).toEqual(["B"]);
  });

  test("the later save succeeding, then the earlier failing, stays saved", async () => {
    const { notes, onSave, settles } = renderOverlappingSaves();

    await act(async () => settles.B?.resolve());
    expect(screen.getByText("Saved")).toBeInTheDocument();
    await act(async () => settles.A?.reject(new Error("refused")));

    expect(screen.getByText("Saved")).toBeInTheDocument();
    // B was accepted, so there is nothing left to send.
    fireEvent.blur(notes);
    expect(onSave).toHaveBeenCalledTimes(2);
  });

  test("the earlier save failing, then the later succeeding, shows saved", async () => {
    const { settles } = renderOverlappingSaves();

    await act(async () => settles.A?.reject(new Error("refused")));
    expect(screen.getByText("Saving…")).toBeInTheDocument();
    await act(async () => settles.B?.resolve());

    expect(screen.getByText("Saved")).toBeInTheDocument();
  });

  test("the later save failing, then the earlier failing, puts back the loaded text", async () => {
    const { notes, onSave, settles } = renderOverlappingSaves();

    // B's failure puts back the text it replaced, "A"; A's later failure
    // then holds its own text again and puts back the loaded "".
    await act(async () => settles.B?.reject(new Error("refused")));
    await act(async () => settles.A?.reject(new Error("refused")));

    expect(screen.getByText("Not saved")).toBeInTheDocument();
    // Neither save was stored, so typing "A" again sends it.
    fireEvent.change(notes, { target: { value: "A" } });
    fireEvent.blur(notes);
    expect(onSave.mock.calls.at(-1)).toEqual(["A"]);
    expect(onSave).toHaveBeenCalledTimes(3);
  });
});
