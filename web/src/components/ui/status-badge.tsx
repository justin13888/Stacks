import type { Status } from "@/api/schemas";
import { cn } from "@/lib/cn";

export const STATUS_LABELS: Record<Status, string> = {
  planning: "Planning",
  watching: "Watching",
  completed: "Completed",
  paused: "Paused",
  dropped: "Dropped",
};

const tones: Record<Status, string> = {
  planning:
    "text-status-planning bg-status-planning/12 ring-status-planning/30",
  watching:
    "text-status-watching bg-status-watching/12 ring-status-watching/30",
  completed:
    "text-status-completed bg-status-completed/12 ring-status-completed/30",
  paused: "text-status-paused bg-status-paused/12 ring-status-paused/30",
  dropped: "text-status-dropped bg-status-dropped/12 ring-status-dropped/30",
};

export interface StatusBadgeProps {
  status: Status;
  /** Marks a rewatch in progress. */
  rewatching?: boolean;
  className?: string;
}

/** A library entry's status, colour-coded with its label always spelled out. */
export function StatusBadge({
  status,
  rewatching = false,
  className,
}: StatusBadgeProps) {
  const label =
    rewatching && status === "watching" ? "Rewatching" : STATUS_LABELS[status];
  return (
    <span
      data-status={status}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset",
        tones[status],
        className,
      )}
    >
      <span aria-hidden className="size-1.5 rounded-full bg-current" />
      {label}
    </span>
  );
}
