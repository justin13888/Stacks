import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { Tooltip, TooltipProvider } from "./tooltip";

describe("Tooltip", () => {
  test("shows on keyboard focus, describes its trigger, and hides on Escape", async () => {
    render(
      <TooltipProvider>
        <Tooltip content="Add to Planning">
          <button type="button">Plan</button>
        </Tooltip>
      </TooltipProvider>,
    );
    await userEvent.tab();
    const trigger = screen.getByRole("button", { name: "Plan" });
    expect(trigger).toHaveFocus();
    const tooltip = await screen.findByRole("tooltip");
    expect(tooltip).toHaveTextContent("Add to Planning");
    expect(trigger).toHaveAccessibleDescription("Add to Planning");

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });
});
