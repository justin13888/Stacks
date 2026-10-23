import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type KeyboardEvent, useState } from "react";
import { describe, expect, test, vi } from "vitest";
import { Combobox } from "./combobox";

const FRUIT = ["Apple", "Banana", "Cherry"] as const;

function Harness({
  onSelect = () => {},
  onItemKeyDown,
  items = FRUIT,
}: {
  onSelect?: (item: string) => void;
  onItemKeyDown?: (
    event: KeyboardEvent<HTMLInputElement>,
    item: string,
  ) => boolean;
  items?: readonly string[];
}) {
  const [text, setText] = useState("");
  const shown = items.filter((f) =>
    f.toLowerCase().includes(text.toLowerCase()),
  );
  return (
    <Combobox
      label="Fruit"
      inputValue={text}
      onInputValueChange={setText}
      items={shown}
      getKey={(f) => f}
      getLabel={(f) => f}
      renderItem={(f) => <span>{f}</span>}
      onSelect={onSelect}
      onItemKeyDown={onItemKeyDown}
      status={<p>Nothing found</p>}
    />
  );
}

describe("Combobox", () => {
  test("wires the combobox, listbox and options together with ARIA", () => {
    render(<Harness />);
    const input = screen.getByRole("combobox", { name: "Fruit" });
    const listbox = screen.getByRole("listbox", { name: "Fruit" });
    expect(input).toHaveAttribute("aria-controls", listbox.id);
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-expanded", "true");

    const options = screen.getAllByRole("option");
    expect(options).toHaveLength(3);
    expect(input).toHaveAttribute("aria-activedescendant", options[0]?.id);
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(options[1]).toHaveAttribute("aria-selected", "false");
  });

  test("arrow keys move the active option and wrap around", async () => {
    render(<Harness />);
    const input = screen.getByRole("combobox", { name: "Fruit" });
    await userEvent.click(input);
    const [apple, banana, cherry] = screen.getAllByRole("option");

    await userEvent.keyboard("{ArrowDown}");
    expect(input).toHaveAttribute("aria-activedescendant", banana?.id);
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(input).toHaveAttribute("aria-activedescendant", apple?.id);
    await userEvent.keyboard("{ArrowUp}");
    expect(input).toHaveAttribute("aria-activedescendant", cherry?.id);
    expect(cherry).toHaveAttribute("aria-selected", "true");
    // Focus never leaves the text field.
    expect(input).toHaveFocus();
  });

  test("Enter picks the active option; a click picks the clicked one", async () => {
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    await userEvent.click(screen.getByRole("combobox", { name: "Fruit" }));
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(onSelect).toHaveBeenLastCalledWith("Banana");

    await userEvent.click(screen.getByRole("option", { name: "Cherry" }));
    expect(onSelect).toHaveBeenLastCalledWith("Cherry");
  });

  test("typing filters and the first match becomes active", async () => {
    render(<Harness />);
    const input = screen.getByRole("combobox", { name: "Fruit" });
    await userEvent.type(input, "{ArrowDown}{ArrowDown}");
    await userEvent.type(input, "an");
    const options = screen.getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["Banana"]);
    expect(input).toHaveAttribute("aria-activedescendant", options[0]?.id);
  });

  test("with no options it collapses and shows its status", async () => {
    render(<Harness />);
    const input = screen.getByRole("combobox", { name: "Fruit" });
    await userEvent.type(input, "zzz");
    expect(input).toHaveAttribute("aria-expanded", "false");
    expect(input).not.toHaveAttribute("aria-activedescendant");
    expect(screen.getByText("Nothing found")).toBeInTheDocument();
  });

  test("item shortcuts act on the active option and stop the default", async () => {
    const onSelect = vi.fn();
    const onItemKeyDown = vi.fn(
      (event: KeyboardEvent<HTMLInputElement>, _item: string) =>
        event.altKey && event.code === "KeyP",
    );
    render(<Harness onSelect={onSelect} onItemKeyDown={onItemKeyDown} />);
    await userEvent.click(screen.getByRole("combobox", { name: "Fruit" }));
    await userEvent.keyboard("{ArrowDown}{Alt>}p{/Alt}");
    expect(onItemKeyDown).toHaveBeenLastCalledWith(expect.anything(), "Banana");
    expect(onSelect).not.toHaveBeenCalled();
  });
});
