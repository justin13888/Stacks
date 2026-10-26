// The title page's dated writes: a movie watched on a past date, and a run
// finished on one. Each is one request that carries the date at the
// precision the user knows it (day, month or year), so the server dates the
// run in the same transaction instead of stamping its own (UTC) today, and
// a failure leaves nothing half-written to retry over.

import { useMutation } from "@tanstack/react-query";
import {
  logPlays,
  updateLibraryEntry,
  updateWatchRun,
} from "@/api/library/library";
import type { RunDate, TitleRef } from "@/api/schemas";
import { reportFailure } from "@/components/ui/error-state";
import { toast } from "@/components/ui/toast";
import { formatRunDate, playTime } from "@/components/ui/when-field";
import { currentRun } from "@/features/library/entry";
import { toastLogged, useEntryCache } from "./use-tracking";

/**
 * Marks a movie watched on `date`: one play then. The server dates the run
 * it lands in: a run it opens starts and finishes on `date`, an open run
 * finishes on it (and starts on it if it started later), and a finished
 * run is a replay that keeps its dates.
 */
export function useMarkWatchedOn(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: async (date: RunDate) => {
      const { plays } = await logPlays(ref, {
        kind: "movie",
        watched_at: playTime(date),
        watched_on: date,
      });
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
 * Finishes run `run` on `date`. The current run is finished by completing
 * the entry on that date, which the server does in one change; an older
 * run only has its completion date set.
 */
export function useFinishRun(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: async ({ run, date }: FinishRequest) => {
      // The optimistic update leaves the runs' numbers as they were.
      const entry = cache.current();
      if (entry && run === currentRun(entry.runs)) {
        await updateLibraryEntry(ref, { status: "completed", on: date });
      } else {
        await updateWatchRun(ref, run, { completed: date });
      }
    },
    onMutate: ({ run, date }) =>
      cache.optimistic((entry) => ({
        ...entry,
        status: run === currentRun(entry.runs) ? "completed" : entry.status,
        runs: entry.runs.map((r) =>
          r.number === run ? { ...r, completed: date } : r,
        ),
      })),
    onSuccess: (_done, { run, date }) => {
      toast({
        tone: "success",
        title: `Run ${run} finished`,
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
