import { Star } from "lucide-react";
import { cn } from "@/lib/cn";
import {
  formatScore,
  maxScore,
  type ScoreFormat,
  toDisplayScore,
} from "@/lib/score-format";

export interface RatingProps {
  /** The stored 0–100 score, or `null` when unscored. */
  score: number | null | undefined;
  /** The user's preferred score format. */
  format: ScoreFormat;
  className?: string;
}

/**
 * A score shown in the user's format: stars for 5-point, a word for 3-point,
 * and a number otherwise. Read-only; entry is a separate control.
 */
export function Rating({ score, format, className }: RatingProps) {
  if (score === null || score === undefined) {
    return (
      <span className={cn("text-sm text-fg-subtle", className)}>Unrated</span>
    );
  }

  const label = `Score: ${formatScore(score, format)}`;

  if (format === "point_5") {
    const filled = toDisplayScore(score, format);
    return (
      <span
        role="img"
        aria-label={label}
        className={cn("inline-flex items-center gap-0.5", className)}
      >
        {Array.from({ length: maxScore(format) }, (_, i) => (
          <Star
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed row of stars
            key={i}
            aria-hidden
            data-filled={i < filled || undefined}
            className={cn(
              "size-4",
              i < filled ? "fill-warning text-warning" : "text-line-strong",
            )}
          />
        ))}
      </span>
    );
  }

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 text-sm font-semibold tabular-nums text-fg",
        className,
      )}
    >
      <Star aria-hidden className="size-4 fill-warning text-warning" />
      <span className="sr-only">Score: </span>
      {formatScore(score, format)}
    </span>
  );
}
