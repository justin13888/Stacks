import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Plus } from "lucide-react";
import { describe, expect, test, vi } from "vitest";
import { Button, IconButton } from "./button";

describe("Button", () => {
  test("is a button that runs its action", async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Save</Button>);
    const button = screen.getByRole("button", { name: "Save" });
    expect(button).toHaveAttribute("type", "button");
    await userEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  test("while loading it is busy and cannot be pressed again", async () => {
    const onClick = vi.fn();
    render(
      <Button loading onClick={onClick}>
        Save
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Save" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("aria-busy", "true");
    await userEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  test("a caller's class overrides the variant's", () => {
    render(<Button className="h-20">Big</Button>);
    const button = screen.getByRole("button", { name: "Big" });
    expect(button.className).toContain("h-20");
    expect(button.className).not.toContain("h-10");
  });
});

describe("IconButton", () => {
  test("is named by its label, not its icon", async () => {
    const onClick = vi.fn();
    render(<IconButton label="Add" icon={<Plus />} onClick={onClick} />);
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
