import { cn } from "@/lib/cn";

export interface ProgressRingProps {
  /** Done so far, e.g. episodes watched. */
  value: number;
  /** The total, e.g. episodes aired; `null` when unknown. */
  max: number | null | undefined;
  /** Diameter in pixels. */
  size?: number;
  /** Names what is measured, e.g. "Episodes watched". */
  label: string;
  /** Show `value` in the middle. */
  showValue?: boolean;
  className?: string;
}

/**
 * Circular progress (a `progressbar`). With no known total it shows the count
 * alone and an open ring, rather than inventing a percentage.
 */
export function ProgressRing({
  value,
  max,
  size = 40,
  label,
  showValue = true,
  className,
}: ProgressRingProps) {
  const stroke = Math.max(3, Math.round(size / 10));
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const known = max !== null && max !== undefined && max > 0;
  const clamped = known
    ? Math.min(Math.max(value, 0), max)
    : Math.max(value, 0);
  const fraction = known ? clamped / max : 0;

  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuenow={clamped}
      aria-valuemax={known ? max : undefined}
      aria-valuetext={known ? `${clamped} of ${max}` : `${clamped}`}
      className={cn("relative inline-flex shrink-0", className)}
      style={{ width: size, height: size }}
    >
      <svg
        aria-hidden
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        className="-rotate-90"
      >
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          strokeWidth={stroke}
          className="stroke-surface-sunken"
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={circumference * (1 - fraction)}
          className={cn(
            "transition-[stroke-dashoffset] duration-(--duration-slow) ease-(--ease-out-soft)",
            fraction >= 1 ? "stroke-success" : "stroke-accent",
          )}
        />
      </svg>
      {showValue && (
        <span
          aria-hidden
          className="absolute inset-0 flex items-center justify-center text-2xs font-semibold tabular-nums text-fg"
        >
          {clamped}
        </span>
      )}
    </div>
  );
}
