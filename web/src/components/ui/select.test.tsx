import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, test, vi } from "vitest";
import { Select } from "./select";

const OPTIONS = [
  { value: "planning", label: "Planning" },
  { value: "watching", label: "Watching" },
  { value: "completed", label: "Completed" },
] as const;

type Value = (typeof OPTIONS)[number]["value"];

function Harness({ onChange }: { onChange: (value: Value) => void }) {
  const [value, setValue] = useState<Value | undefined>(undefined);
  return (
    <Select
      label="Status"
      value={value}
      options={OPTIONS}
      onValueChange={(next) => {
        setValue(next);
        onChange(next);
      }}
    />
  );
}

describe("Select", () => {
  test("is a labelled combobox that opens a listbox and picks an option", async () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    const trigger = screen.getByRole("combobox", { name: "Status" });
    expect(trigger).toHaveTextContent("Choose…");

    await userEvent.click(trigger);
    const listbox = await screen.findByRole("listbox");
    expect(listbox).toBeVisible();
    expect(screen.getAllByRole("option")).toHaveLength(3);

    await userEvent.click(screen.getByRole("option", { name: "Watching" }));
    expect(onChange).toHaveBeenCalledWith("watching");
    expect(trigger).toHaveTextContent("Watching");
  });

  test("works from the keyboard", async () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    screen.getByRole("combobox", { name: "Status" }).focus();
    await userEvent.keyboard("{Enter}");
    await screen.findByRole("listbox");
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(onChange).toHaveBeenCalledOnce();
  });
});
