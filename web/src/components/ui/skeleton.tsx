import { cn } from "@/lib/cn";

export interface SkeletonProps {
  className?: string;
  /** Round, for avatars and rings. */
  circle?: boolean;
}

/** A shimmering placeholder for content that is loading. Purely decorative. */
export function Skeleton({ className, circle = false }: SkeletonProps) {
  return (
    <div
      aria-hidden
      data-skeleton
      className={cn(
        "animate-shimmer bg-[linear-gradient(90deg,var(--color-surface-sunken)_0%,var(--color-surface-raised)_50%,var(--color-surface-sunken)_100%)] bg-size-[200%_100%]",
        circle ? "rounded-full" : "rounded-md",
        className,
      )}
    />
  );
}
