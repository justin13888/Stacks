import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import { Toaster, toast } from "./toast";

describe("Toast", () => {
  test("toast() from outside React shows it in the notifications region", async () => {
    render(<Toaster />);
    act(() => {
      toast({
        title: "Added to Planning",
        description: "Dune",
        tone: "success",
      });
    });
    const region = screen.getByRole("region", { name: "Notifications (F8)" });
    const [item] = within(region).getAllByRole("listitem");
    expect(item).toHaveTextContent("Added to Planning");
    expect(item).toHaveTextContent("Dune");
  });

  test("errors interrupt (assertive); confirmations wait (polite)", () => {
    render(<Toaster />);
    act(() => {
      toast({ title: "Saved", tone: "success" });
      toast({ title: "Couldn't save", tone: "error" });
    });
    // Each toast is announced through its own live region.
    const live = screen
      .getAllByRole("status")
      .map((s) => s.getAttribute("aria-live"));
    expect(live).toEqual(["polite", "assertive"]);
  });

  test("F8 moves focus to the notifications; the close button dismisses", async () => {
    render(<Toaster />);
    act(() => {
      toast({ title: "Marked as watched" });
    });
    fireEvent.keyDown(document, { key: "F8", code: "F8" });
    const region = screen.getByRole("region", { name: "Notifications (F8)" });
    expect(region).toContainElement(document.activeElement as HTMLElement);

    await userEvent.click(
      screen.getByRole("button", { name: "Dismiss notification" }),
    );
    expect(screen.queryByText("Marked as watched")).not.toBeInTheDocument();
  });

  test("an action runs its handler", async () => {
    const undo = vi.fn();
    render(<Toaster />);
    act(() => {
      toast({ title: "Removed", action: { label: "Undo", onClick: undo } });
    });
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(undo).toHaveBeenCalledOnce();
  });
});
