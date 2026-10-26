import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, test, vi } from "vitest";
import {
  formatRunDate,
  playTime,
  runDateBefore,
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

  test("a play logged for today is logged now, never later today", () => {
    const morning = new Date(2026, 9, 26, 8, 30);
    expect(playTime({ date: "2026-10-26", precision: "day" }, morning)).toBe(
      morning.toISOString(),
    );
    // Yesterday, or this month as a whole, is still noon on its first day.
    const yesterday = new Date(
      playTime({ date: "2026-10-25", precision: "day" }, morning),
    );
    expect([yesterday.getDate(), yesterday.getHours()]).toEqual([25, 12]);
    const month = new Date(
      playTime({ date: "2026-10-01", precision: "month" }, morning),
    );
    expect([month.getDate(), month.getHours()]).toEqual([1, 12]);
  });

  test("dates compare only as far as both are known", () => {
    const d = (date: string, precision: WhenValue["precision"]) => ({
      date,
      precision,
    });
    expect(runDateBefore(d("2019-03-04", "day"), d("2019-03-05", "day"))).toBe(
      true,
    );
    expect(runDateBefore(d("2019-01-01", "year"), d("2019-06-05", "day"))).toBe(
      false,
    );
    expect(runDateBefore(d("2019-06-05", "day"), d("2019-01-01", "year"))).toBe(
      false,
    );
    expect(
      runDateBefore(d("2019-05-01", "month"), d("2019-06-01", "day")),
    ).toBe(true);
  });

  test("a date before its bound is refused, saying why", () => {
    const bound = {
      date: { date: "2020-05-17", precision: "day" as const },
      reason: "the run started",
    };
    const at = (day: string, precision: WhenValue["precision"]) =>
      whenError({ day, precision }, NOW, bound);
    expect(at("2020-05-16", "day")).toMatch(
      /^That's before the run started \(/,
    );
    expect(at("2020-05-17", "day")).toBeNull();
    // May 2020, or 2020, may well be after the 17th.
    expect(at("2020-05-01", "month")).toBeNull();
    expect(at("2020-01-01", "year")).toBeNull();
    expect(at("2020-04-30", "month")).not.toBeNull();
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
