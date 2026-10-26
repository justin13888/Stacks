import { describe, expect, test } from "vitest";
import type { RunDate, WatchRun } from "@/api/schemas";
import { today } from "@/lib/format";
import { movieRunChanges, openedToday } from "./use-run-dates";

const MARCH: RunDate = { date: "2019-03-01", precision: "month" };
const day = (date: string): RunDate => ({ date, precision: "day" });

describe("the run dates a dated movie play gives", () => {
  test("no run yet: run 1 starts and ends on the date", () => {
    expect(movieRunChanges([], MARCH)).toEqual({
      run: 1,
      changes: { started: MARCH, completed: MARCH },
    });
  });

  test("an open run ends on the date, keeping an earlier start", () => {
    const runs: WatchRun[] = [
      { number: 1, started: day("2018-12-24"), completed: null },
    ];
    expect(movieRunChanges(runs, MARCH)).toEqual({
      run: 1,
      changes: { completed: MARCH },
    });
  });

  test("an open run that started later, or never said, starts on it too", () => {
    for (const started of [day("2026-10-01"), null]) {
      const runs: WatchRun[] = [
        { number: 1, started: day("2010-01-01"), completed: day("2010-01-02") },
        { number: 2, started, completed: null },
      ];
      expect(movieRunChanges(runs, MARCH)).toEqual({
        run: 2,
        changes: { started: MARCH, completed: MARCH },
      });
    }
  });

  test("a finished run is a replay and keeps its dates", () => {
    const runs: WatchRun[] = [
      { number: 1, started: null, completed: day("2020-05-01") },
    ];
    expect(movieRunChanges(runs, MARCH)).toBeNull();
  });

  test("a title not in the library dates only the run the play opened", () => {
    const now = day(today());
    expect(
      openedToday([{ number: 1, started: now, completed: now }], MARCH),
    ).toEqual({ run: 1, changes: { started: MARCH, completed: MARCH } });
    // Restored from the trash with its own history: left as it was.
    expect(
      openedToday(
        [{ number: 1, started: day("2015-01-01"), completed: now }],
        MARCH,
      ),
    ).toBeNull();
    expect(
      openedToday(
        [
          { number: 1, started: now, completed: now },
          { number: 2, started: now, completed: now },
        ],
        MARCH,
      ),
    ).toBeNull();
  });
});
