import { Link } from "@tanstack/react-router";
import { Heart, Plus } from "lucide-react";
import type { LibraryEntry } from "@/api/schemas";
import { IconButton } from "@/components/ui/button";
import { Poster } from "@/components/ui/poster";
import { ProgressRing } from "@/components/ui/progress-ring";
import { Rating } from "@/components/ui/rating";
import { StatusBadge } from "@/components/ui/status-badge";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { cn } from "@/lib/cn";
import { useQuickLog } from "./use-library";

/** Whether "+1" applies: a show not finished, or a movie not yet watched. */
export function canQuickLog(entry: LibraryEntry): boolean {
  if (entry.status === "completed" || entry.status === "dropped") return false;
  return entry.title.kind === "show" || entry.progress.current_run_plays === 0;
}

/**
 * Progress in the current run: the episodes watched, as a fraction of the
 * show's total when the title carries one (the count alone otherwise).
 */
export function EntryProgress({
  entry,
  size = 32,
}: {
  entry: LibraryEntry;
  size?: number;
}) {
  if (entry.title.kind === "movie") return null;
  return (
    <ProgressRing
      value={entry.progress.current_run_episodes}
      max={entry.title.episode_count}
      size={size}
      label={`Episodes watched of ${entry.title.title}`}
    />
  );
}

export function QuickLogButton({
  entry,
  className,
}: {
  entry: LibraryEntry;
  className?: string;
}) {
  const quick = useQuickLog();
  if (!canQuickLog(entry)) return null;
  const label =
    entry.title.kind === "movie"
      ? `Mark ${entry.title.title} watched`
      : `Log the next episode of ${entry.title.title}`;
  return (
    <IconButton
      label={label}
      variant="secondary"
      size="sm"
      icon={<Plus aria-hidden className="size-4" />}
      disabled={quick.isPending}
      aria-busy={quick.isPending || undefined}
      onClick={() => quick.mutate(entry.title)}
      className={className}
    />
  );
}

/** A poster card for the grid view. */
export function EntryCard({ entry }: { entry: LibraryEntry }) {
  const { title_language, score_format } = usePreferences();
  const name = displayTitle(entry.title, title_language);
  return (
    <article aria-label={name} className="group relative flex flex-col gap-2">
      <Link
        to="/titles/$ref"
        params={{ ref: entry.title.ref }}
        className="flex flex-col gap-2 rounded-poster focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      >
        <Poster
          src={entry.title.poster_url}
          title={name}
          className="w-full transition-transform duration-(--duration-base) group-hover:-translate-y-0.5"
        />
        <span className="line-clamp-2 text-sm font-medium text-fg">{name}</span>
      </Link>
      <div className="flex items-center gap-2">
        <StatusBadge
          status={entry.status}
          rewatching={entry.progress.runs > 1 && entry.status === "watching"}
        />
        {entry.favourite && (
          <Heart
            aria-label="Favourite"
            className="size-4 fill-danger text-danger"
          />
        )}
      </div>
      <div className="flex items-center justify-between gap-2">
        <Rating score={entry.score} format={score_format} className="text-xs" />
        <div className={cn("flex items-center gap-1.5")}>
          <EntryProgress entry={entry} />
          <QuickLogButton entry={entry} />
        </div>
      </div>
    </article>
  );
}
