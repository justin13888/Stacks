import { describe, expect, test } from "vitest";
import { formatYearMonth, humanizeMinutes } from "./format";

describe("humanizeMinutes", () => {
  test("returns '0 min' for zero and negative inputs", () => {
    expect(humanizeMinutes(0)).toBe("0 min");
    expect(humanizeMinutes(-10)).toBe("0 min");
    expect(humanizeMinutes(Number.NaN)).toBe("0 min");
  });

  test("returns minutes for sub-hour values", () => {
    expect(humanizeMinutes(1)).toBe("1 min");
    expect(humanizeMinutes(45)).toBe("45 min");
    expect(humanizeMinutes(59)).toBe("59 min");
  });

  test("returns hours and minutes for sub-day values", () => {
    expect(humanizeMinutes(60)).toBe("1h");
    expect(humanizeMinutes(75)).toBe("1h 15m");
    expect(humanizeMinutes(120)).toBe("2h");
    expect(humanizeMinutes(150)).toBe("2h 30m");
  });

  test("returns days for >= 24h, collapsing zero segments", () => {
    expect(humanizeMinutes(1440)).toBe("1d");
    expect(humanizeMinutes(1500)).toBe("1d 1h");
    // The minutes segment is dropped once days are present
    expect(humanizeMinutes(1505)).toBe("1d 1h");
    expect(humanizeMinutes(48 * 60)).toBe("2d");
  });

  test("rounds fractional minutes", () => {
    expect(humanizeMinutes(59.4)).toBe("59 min");
    expect(humanizeMinutes(59.6)).toBe("1h");
  });
});

describe("formatYearMonth", () => {
  test("formats a YYYY-MM string", () => {
    expect(formatYearMonth("2026-01")).toMatch(/Jan\s+2026/);
    expect(formatYearMonth("2026-12")).toMatch(/Dec\s+2026/);
  });

  test("returns the input unchanged when malformed", () => {
    expect(formatYearMonth("nope")).toBe("nope");
    expect(formatYearMonth("2026-13")).toBe("2026-13");
  });
});
