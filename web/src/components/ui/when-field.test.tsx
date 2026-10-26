import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, test, vi } from "vitest";
import {
  formatRunDate,
  playTime,
  toRunDate,
  type WhenValue,
  WhenField,
  whenError,
} from "./when-field";

const NOW = new Date(2026, 9, 26, 15, 0);

describe("when-field values", () => {
  test("a run date keeps only what its precision knows", () => {
    expect(toRunDate({ day: "2019-03-17", precision: "day" })).toEqual({
      date: "2019-03-17",
      precision: "day",
    });
    expect(toRunDate({ day: "2019-03-17", precision: "month" })).toEqual({
      date: "2019-03-01",
      precision: "month",
    });
    expect(toRunDate({ day: "2019-03-17", precision: "year" })).toEqual({
      date: "2019-01-01",
      precision: "year",
    });
  });

  test("a play is logged at local noon on the date's first day", () => {
    const at = new Date(playTime({ date: "2019-03-01", precision: "month" }));
    expect([at.getFullYear(), at.getMonth(), at.getDate()]).toEqual([
      2019, 2, 1,
    ]);
    expect(at.getHours()).toBe(12);
  });

  test("the future is refused only beyond what the precision knows", () => {
    const at = (day: string, precision: WhenValue["precision"]) =>
      whenError({ day, precision }, NOW);
    expect(at("2026-10-26", "day")).toBeNull();
    expect(at("2026-10-27", "day")).toBe("That's in the future.");
    // Later this month, but "this month" is not the future.
    expect(at("2026-10-30", "month")).toBeNull();
    expect(at("2026-11-01", "month")).toBe("That's in the future.");
    expect(at("2026-12-31", "year")).toBeNull();
    expect(at("2027-01-01", "year")).toBe("That's in the future.");
    expect(at("1899-12-31", "year")).toBe("Choose a date from 1900 on.");
  });

  test("a run date reads as precisely as it is known", () => {
    expect(formatRunDate({ date: "2019-01-01", precision: "year" })).toBe(
      "2019",
    );
    expect(formatRunDate({ date: "2019-03-01", precision: "month" })).toMatch(
      /Mar/,
    );
    expect(formatRunDate({ date: "2019-03-17", precision: "day" })).toMatch(
      /17/,
    );
    expect(formatRunDate(null)).toBeNull();
  });
});

function Harness({ onChange }: { onChange: (value: WhenValue) => void }) {
  const [value, setValue] = useState<WhenValue>({
    day: "2024-01-31",
    precision: "day",
  });
  return (
    <WhenField
      label="Watched"
      value={value}
      onChange={(next) => {
        setValue(next);
        onChange(next);
      }}
    />
  );
}

describe("WhenField", () => {
  test("picks a day, then a month of a year, then a year alone", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onChange={onChange} />);
    expect(screen.getByRole("group", { name: "Watched" })).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Date"), {
      target: { value: "2023-05-20" },
    });
    expect(onChange).toHaveBeenLastCalledWith({
      day: "2023-05-20",
      precision: "day",
    });

    await user.click(screen.getByRole("radio", { name: "Month" }));
    expect(screen.queryByLabelText("Date")).not.toBeInTheDocument();
    await user.click(screen.getByRole("combobox", { name: "Month" }));
    await user.click(await screen.findByRole("option", { name: "February" }));
    // The 20th fits February; the day is kept underneath.
    expect(onChange).toHaveBeenLastCalledWith({
      day: "2023-02-20",
      precision: "month",
    });

    const year = screen.getByLabelText("Year");
    await user.clear(year);
    await user.type(year, "20");
    // An unfinished year changes nothing yet.
    expect(onChange).toHaveBeenLastCalledWith({
      day: "2023-02-20",
      precision: "month",
    });
    await user.type(year, "21");
    expect(onChange).toHaveBeenLastCalledWith({
      day: "2021-02-20",
      precision: "month",
    });

    await user.click(screen.getByRole("radio", { name: "Year" }));
    expect(
      screen.queryByRole("combobox", { name: "Month" }),
    ).not.toBeInTheDocument();
    expect(screen.getByLabelText("Year")).toHaveValue(2021);
  });

  test("a month too short for the day clamps it", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onChange={onChange} />);
    await user.click(screen.getByRole("radio", { name: "Month" }));
    await user.click(screen.getByRole("combobox", { name: "Month" }));
    await user.click(await screen.findByRole("option", { name: "February" }));
    expect(onChange).toHaveBeenLastCalledWith({
      day: "2024-02-29",
      precision: "month",
    });
  });

  test("the precision is chosen from the keyboard", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onChange={onChange} />);
    screen.getByRole("radio", { name: "Day" }).focus();
    await user.keyboard("{ArrowRight}{ArrowRight}{Enter}");
    expect(onChange).toHaveBeenLastCalledWith({
      day: "2024-01-31",
      precision: "year",
    });
  });
});
