import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { Sheet, SheetContent, SheetTrigger } from "./sheet";

describe("Sheet", () => {
  test("opens a named dialog on the requested edge and closes on Escape", async () => {
    render(
      <Sheet>
        <SheetTrigger>More</SheetTrigger>
        <SheetContent side="bottom" title="More sections">
          <a href="/stats">Stats</a>
        </SheetContent>
      </Sheet>,
    );
    const trigger = screen.getByRole("button", { name: "More" });
    await userEvent.click(trigger);
    const sheet = screen.getByRole("dialog", { name: "More sections" });
    expect(sheet).toHaveAttribute("data-side", "bottom");
    expect(screen.getByRole("link", { name: "Stats" })).toBeVisible();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
});
