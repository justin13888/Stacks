import { CloudOff } from "lucide-react";
import { type FreshnessTracker, useHasStaleData } from "@/lib/freshness";

/**
 * Says so while any data on screen came from the stored snapshot because a
 * metadata provider failed (`stale: true`, or a non-empty `degraded`).
 */
export function StaleBanner({ freshness }: { freshness: FreshnessTracker }) {
  const stale = useHasStaleData(freshness);
  if (!stale) return null;
  return (
    <div
      role="status"
      className="flex items-center gap-2 border-b border-line bg-warning/12 px-4 py-2 text-sm text-fg"
    >
      <CloudOff aria-hidden className="size-4 shrink-0 text-warning" />
      <span>
        A metadata provider isn't answering, so some details are from a saved
        copy and may be out of date.
      </span>
    </div>
  );
}
