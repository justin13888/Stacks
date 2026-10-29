// Score display in the user's format.
//
// The API stores every score as an integer 0–100; the user's preference
// (`preferences.score_format`) only decides how it is shown and entered.

import type { ScoreFormat } from "@/api/schemas";

export type { ScoreFormat };

/** The largest value a format shows. */
export function maxScore(format: ScoreFormat): number {
  switch (format) {
    case "point_100":
      return 100;
    case "point_10_decimal":
    case "point_10":
      return 10;
    case "point_5":
      return 5;
    case "point_3":
      return 3;
  }
}

/** A stored 0–100 score in the format's own units. */
export function toDisplayScore(score: number, format: ScoreFormat): number {
  const clamped = Math.min(100, Math.max(0, score));
  switch (format) {
    case "point_100":
      return Math.round(clamped);
    case "point_10_decimal":
      return Math.round(clamped) / 10;
    case "point_10":
      return Math.round(clamped / 10);
    case "point_5":
      return Math.round(clamped / 20);
    case "point_3":
      // Thirds of the range: 1 = disliked, 2 = okay, 3 = liked.
      return Math.min(3, Math.max(1, Math.ceil(clamped / 33.4)));
  }
}

/** A value entered in the format's units, as the stored 0–100 integer. */
export function fromDisplayScore(value: number, format: ScoreFormat): number {
  const scaled = (value / maxScore(format)) * 100;
  return Math.round(Math.min(100, Math.max(0, scaled)));
}

/** Every stored score, 0 to 100. */
const STORED_SCORES = Array.from({ length: 101 }, (_, i) => i);

/**
 * A bound entered in the format's units as the stored 0–100 bound that keeps
 * every score shown as that value: the lowest stored score shown as at least
 * `value` for a minimum, the highest shown as at most `value` for a maximum.
 * So in `point_10` a minimum of 9 is 85, the lowest score shown as 9 / 10.
 */
export function storedBound(
  value: number,
  format: ScoreFormat,
  side: "min" | "max",
): number {
  const found =
    side === "min"
      ? STORED_SCORES.find((s) => toDisplayScore(s, format) >= value)
      : STORED_SCORES.findLast((s) => toDisplayScore(s, format) <= value);
  return found ?? fromDisplayScore(value, format);
}

const POINT_3_LABELS = ["Disliked", "Okay", "Liked"] as const;

/** The score as text, e.g. `8.5`, `85`, `4 / 5`, `Liked`. */
export function formatScore(score: number, format: ScoreFormat): string {
  const value = toDisplayScore(score, format);
  switch (format) {
    case "point_100":
      return String(value);
    case "point_10_decimal":
      return value.toFixed(1);
    case "point_10":
      return `${value} / 10`;
    case "point_5":
      return `${value} / 5`;
    case "point_3":
      return POINT_3_LABELS[value - 1] ?? "Okay";
  }
}
