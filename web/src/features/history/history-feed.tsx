import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  CirclePlay,
  History as HistoryIcon,
  PencilLine,
  RotateCcw,
  Undo2,
} from "lucide-react";
import { useState } from "react";
import { updateLibraryEntry, voidPlay } from "@/api/library/library";
import type { EntryEvent, TitleSummary } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { Poster } from "@/components/ui/poster";
import { toast } from "@/components/ui/toast";
import { formatDay, toDay, today } from "@/features/calendar/dates";
import { invalidateTracking } from "@/features/library/entry";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { reportFailure } from "@/features/titles/use-tracking";
import { type Described, describeChange, type Undo } from "./describe";

/** One event, with the title it concerns. */
export interface FeedEvent {
  title: Pick<TitleSummary, "ref" | "title" | "original_title" | "poster_url">;
  event: EntryEvent;
}

interface UndoRequest {
  ref: string;
  eventId: string;
  undo: Undo;
}

/** Applies an event's inverse: voids its play, or patches the entry back. */
function useUndoEvent() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ ref, undo }: UndoRequest) => {
      if (undo.kind === "void-play") await voidPlay(undo.playId);
      else await updateLibraryEntry(ref, undo.changes);
    },
    onSuccess: () => toast({ tone: "success", title: "Undone" }),
    onError: reportFailure,
    onSettled: (_void, _error, { ref }) => invalidateTracking(queryClient, ref),
  });
}

function dayHeading(day: string): string {
  const now = new Date();
  if (day === today(now)) return "Today";
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (day === toDay(yesterday)) return "Yesterday";
  return formatDay(day, {
    weekday: "long",
    month: "long",
    day: "numeric",
    year: "numeric",
  });
}

function timeOf(instant: string): string {
  return new Date(instant).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
}

const ICONS = {
  play_logged: CirclePlay,
  play_voided: Undo2,
  entry_updated: PencilLine,
} as const;

/**
 * Events grouped by the day they happened on, newest first, each in words
 * and with an undo where one exists. `showTitle` adds each event's poster
 * (the global feed); a title's own feed leaves it out.
 */
export function HistoryFeed({
  events,
  showTitle,
}: {
  events: readonly FeedEvent[];
  showTitle: boolean;
}) {
  const { title_language, score_format } = usePreferences();
  const undo = useUndoEvent();
  const [undone, setUndone] = useState<ReadonlySet<string>>(new Set());

  // A play whose latest loaded void or restoration (events run newest first)
  // is a void has nothing left to undo; one restored since can be undone again.
  const voided = new Set<string>();
  const settled = new Set<string>();
  for (const { event } of events) {
    const { change } = event;
    if (change.kind !== "play_voided" && change.kind !== "play_restored")
      continue;
    if (settled.has(change.play.id)) continue;
    settled.add(change.play.id);
    if (change.kind === "play_voided") voided.add(change.play.id);
  }

  const days = new Map<string, FeedEvent[]>();
  for (const item of events) {
    const day = toDay(new Date(item.event.at));
    const group = days.get(day);
    if (group) group.push(item);
    else days.set(day, [item]);
  }

  return (
    <div className="flex flex-col gap-6">
      {[...days.entries()].map(([day, items]) => {
        const headingId = `history-${day}`;
        return (
          <section key={day} aria-labelledby={headingId}>
            <h2
              id={headingId}
              className="mb-2 text-sm font-semibold text-fg-muted"
            >
              {dayHeading(day)}
            </h2>
            <ol className="flex flex-col divide-y divide-line rounded-xl border border-line bg-surface">
              {items.map(({ title, event }) => {
                const name = displayTitle(title, title_language);
                const described: Described = describeChange(
                  event.change,
                  name,
                  score_format,
                  voided,
                );
                const Icon =
                  ICONS[event.change.kind as keyof typeof ICONS] ?? HistoryIcon;
                const canUndo = described.undo && !undone.has(event.id);
                const pending =
                  undo.isPending && undo.variables?.eventId === event.id;
                return (
                  <li key={event.id} className="flex items-center gap-3 p-3">
                    {showTitle ? (
                      <Link
                        to="/titles/$ref"
                        params={{ ref: title.ref }}
                        tabIndex={-1}
                        aria-hidden
                      >
                        <Poster src={title.poster_url} title={name} size="xs" />
                      </Link>
                    ) : (
                      <Icon aria-hidden className="size-5 text-fg-subtle" />
                    )}
                    <div className="flex min-w-0 flex-1 flex-col">
                      {described.lines.map((line) => (
                        <span key={line} className="text-sm text-fg">
                          {line}
                        </span>
                      ))}
                      <time
                        dateTime={event.at}
                        className="text-xs text-fg-subtle"
                      >
                        {timeOf(event.at)}
                      </time>
                    </div>
                    {canUndo && described.undo && (
                      <Button
                        variant="ghost"
                        size="sm"
                        loading={pending}
                        aria-label={`Undo: ${described.lines.join("; ")}`}
                        onClick={() => {
                          const request = {
                            ref: title.ref,
                            eventId: event.id,
                            undo: described.undo as Undo,
                          };
                          undo.mutate(request, {
                            onSuccess: () =>
                              setUndone((prev) => new Set(prev).add(event.id)),
                          });
                        }}
                      >
                        <RotateCcw aria-hidden className="size-4" />
                        Undo
                      </Button>
                    )}
                  </li>
                );
              })}
            </ol>
          </section>
        );
      })}
    </div>
  );
}
