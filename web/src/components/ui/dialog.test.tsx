import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { Button } from "./button";
import { Dialog, DialogClose, DialogContent, DialogTrigger } from "./dialog";

function Harness() {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button>Delete list</Button>
      </DialogTrigger>
      <DialogContent
        title="Delete this list?"
        description="Its titles stay in your library."
      >
        <DialogClose asChild>
          <Button variant="danger">Delete</Button>
        </DialogClose>
      </DialogContent>
    </Dialog>
  );
}

describe("Dialog", () => {
  test("is a modal dialog named by its title and described by its description", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("button", { name: "Delete list" }));
    const dialog = screen.getByRole("dialog", { name: "Delete this list?" });
    expect(dialog).toHaveAccessibleDescription(
      "Its titles stay in your library.",
    );
    // Focus moves into the dialog.
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
  });

  test("Escape closes it and returns focus to the trigger", async () => {
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "Delete list" });
    await userEvent.click(trigger);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  test("Tab stays inside the dialog", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("button", { name: "Delete list" }));
    const dialog = screen.getByRole("dialog");
    for (let i = 0; i < 4; i++) {
      await userEvent.tab();
      expect(dialog).toContainElement(document.activeElement as HTMLElement);
    }
  });

  test("the corner button closes it", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("button", { name: "Delete list" }));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
