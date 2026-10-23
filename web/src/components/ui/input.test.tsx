import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { Input } from "./input";

describe("Input", () => {
  test("is labelled by its label and takes typing", async () => {
    render(<Input label="Display name" hint="Shown to you only" />);
    const input = screen.getByRole("textbox", { name: "Display name" });
    expect(input).toHaveAccessibleDescription("Shown to you only");
    await userEvent.type(input, "Ada");
    expect(input).toHaveValue("Ada");
  });

  test("an error marks it invalid and describes it", () => {
    render(<Input label="Name" error="Required" />);
    const input = screen.getByRole("textbox", { name: "Name" });
    expect(input).toBeInvalid();
    expect(input).toHaveAccessibleDescription("Required");
  });

  test("without a label it renders the bare field", () => {
    render(<Input aria-label="Filter" />);
    expect(screen.getByRole("textbox", { name: "Filter" })).toBeValid();
  });
});
