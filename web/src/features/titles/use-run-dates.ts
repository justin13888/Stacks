// The title page's dated writes: a movie watched on a past date, and a run
// finished on one. Each records the date at the precision the user knows it
// (day, month or year) on the run, which is where the API keeps precision.

import { useMutation } from "@tanstack/react-query";
import {
  getLibraryEntry,
  logPlays,
  updateLibraryEntry,
  updateWatchRun,
} from "@/api/library/library";
import type { RunChanges, RunDate, TitleRef, WatchRun } from "@/api/schemas";
import { reportFailure } from "@/components/ui/error-state";
import { toast } from "@/components/ui/toast";
import { formatRunDate, playTime } from "@/components/ui/when-field";
import { currentRun } from "@/features/library/entry";
import { today } from "@/lib/format";
import { toastLogged, useEntryCache } from "./use-tracking";

/**
 * The dates a movie play on `date` gives the run it lands in. A first watch
 * (no run yet, so the server opens one today) starts and ends on `date`; an
 * open run ends on it, and starts on it too if it started later or never
 * said when. A run already finished keeps its dates: this is a replay.
 */
export function movieRunChanges(
  runs: readonly WatchRun[],
  date: RunDate,
): { run: number; changes: RunChanges } | null {
  const run = runs.find((r) => r.number === currentRun(runs));
  if (!run) return { run: 1, changes: { started: date, completed: date } };
  if (run.completed) return null;
  const startsLater = !run.started || run.started.date > date.date;
  return {
    run: run.number,
    changes: startsLater
      ? { started: date, completed: date }
      : { completed: date },
  };
}

/**
 * For a title that was not in the library: the play created its entry, or
 * brought it back from the trash. Only a run the play itself opened (the
 * one run, started and finished today) takes `date`; a restored entry's
 * history is left as it was.
 */
function openedToday(
  runs: readonly WatchRun[],
  date: RunDate,
): { run: number; changes: RunChanges } | null {
  const [only, ...rest] = runs;
  const day = today();
  if (!only || rest.length > 0) return null;
  if (only.started?.date !== day || only.completed?.date !== day) return null;
  return { run: only.number, changes: { started: date, completed: date } };
}

/** Marks a movie watched on `date`: one play then, and the run's dates. */
export function useMarkWatchedOn(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: async (date: RunDate) => {
      const before = cache.current();
      const { plays } = await logPlays(ref, {
        kind: "movie",
        watched_at: playTime(date),
      });
      const dates = before
        ? movieRunChanges(before.runs, date)
        : openedToday((await getLibraryEntry(ref)).runs, date);
      if (dates) await updateWatchRun(ref, dates.run, dates.changes);
      return plays;
    },
    onSuccess: (plays, date) =>
      toastLogged(
        plays,
        "Marked as watched",
        () => cache.settle(),
        `Watched ${formatRunDate(date)}`,
      ),
    onError: reportFailure,
    onSettled: () => cache.settle(),
  });
}

export interface FinishRequest {
  run: number;
  date: RunDate;
}

/**
 * Finishes run `run` on `date`. Finishing the current run also completes
 * the entry, which the server would stamp with today; the date then
 * replaces that stamp.
 */
export function useFinishRun(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: async ({ run, date }: FinishRequest) => {
      const entry = cache.current();
      const isCurrent = entry ? run === currentRun(entry.runs) : false;
      if (isCurrent && entry?.status !== "completed") {
        await updateLibraryEntry(ref, { status: "completed" });
      }
      return updateWatchRun(ref, run, { completed: date });
    },
    onMutate: ({ run, date }) =>
      cache.optimistic((entry) => ({
        ...entry,
        status: run === currentRun(entry.runs) ? "completed" : entry.status,
        runs: entry.runs.map((r) =>
          r.number === run ? { ...r, completed: date } : r,
        ),
      })),
    onSuccess: (run, { date }) => {
      toast({
        tone: "success",
        title: `Run ${run.number} finished`,
        description: `Finished ${formatRunDate(date)}`,
      });
    },
    onError: (error, _request, snapshot) => {
      cache.rollback(snapshot);
      reportFailure(error);
    },
    onSettled: () => cache.settle(),
  });
}
