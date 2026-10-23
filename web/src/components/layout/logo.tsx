import { cn } from "@/lib/cn";

/** The Stacks mark and wordmark. The mark matches the PWA icon. */
export function Logo({
  className,
  compact = false,
}: {
  className?: string;
  compact?: boolean;
}) {
  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      <svg
        aria-hidden
        viewBox="0 0 32 32"
        className="size-7"
        role="presentation"
      >
        <rect width="32" height="32" rx="8" className="fill-accent" />
        <rect
          x="8"
          y="8"
          width="16"
          height="4"
          rx="2"
          fill="white"
          opacity="0.55"
        />
        <rect
          x="8"
          y="14"
          width="16"
          height="4"
          rx="2"
          fill="white"
          opacity="0.8"
        />
        <rect x="8" y="20" width="16" height="4" rx="2" fill="white" />
      </svg>
      {!compact && (
        <span className="font-display text-lg font-bold tracking-tight text-fg">
          Stacks
        </span>
      )}
    </span>
  );
}
