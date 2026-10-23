import { describe, expect, test } from "vitest";
import {
  formatScore,
  fromDisplayScore,
  maxScore,
  toDisplayScore,
} from "./score-format";

describe("score formats", () => {
  test.each([
    ["point_100", 85, "85"],
    ["point_10_decimal", 85, "8.5"],
    ["point_10", 85, "9 / 10"],
    ["point_5", 85, "4 / 5"],
    ["point_3", 85, "Liked"],
    ["point_3", 50, "Okay"],
    ["point_3", 10, "Disliked"],
  ] as const)("%s shows %i as %s", (format, score, text) => {
    expect(formatScore(score, format)).toBe(text);
  });

  test("a display value maps back into 0–100", () => {
    expect(fromDisplayScore(8.5, "point_10_decimal")).toBe(85);
    expect(fromDisplayScore(4, "point_5")).toBe(80);
    expect(fromDisplayScore(3, "point_3")).toBe(100);
    expect(fromDisplayScore(12, "point_10")).toBe(100);
  });

  test("stored scores outside 0–100 are clamped", () => {
    expect(toDisplayScore(140, "point_100")).toBe(100);
    expect(toDisplayScore(-5, "point_10_decimal")).toBe(0);
    expect(maxScore("point_3")).toBe(3);
  });
});
