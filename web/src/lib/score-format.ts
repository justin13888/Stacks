/**
 * Score format conversion.
 * Internal storage: 0–100 integer.
 * Display formats: 5-star (0–5, 0.5 increments), 10-point (0–10, 0.1 increments), 100-point (0–100).
 */

export type ScoreFormat = "5-star" | "10-point" | "100-point";

/** Convert internal 0–100 score to the display format. */
export function formatScore(
  score: number,
  format: ScoreFormat,
): number | string {
  if (format === "100-point") return score;
  if (format === "10-point") return Math.round((score / 10) * 10) / 10;
  if (format === "5-star") return Math.round((score / 20) * 2) / 2;
  return score;
}

/** Convert a display-format score back to the internal 0–100 integer. */
export function parseScore(value: number, format: ScoreFormat): number {
  if (format === "100-point") return Math.round(value);
  if (format === "10-point") return Math.round(value * 10);
  if (format === "5-star") return Math.round(value * 20);
  return Math.round(value);
}

/** Maximum display value for a given format. */
export function maxScore(format: ScoreFormat): number {
  if (format === "100-point") return 100;
  if (format === "10-point") return 10;
  if (format === "5-star") return 5;
  return 100;
}
