// The title page's writes. Each one updates the cached entry at once
// (optimistically), puts it back if the server refuses, and then refetches
// everything the change can alter.

import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  getGetLibraryEntryQueryKey,
  logPlays,
  restorePlay,
  startRewatch,
  updateLibraryEntry,
  voidPlay,
} from "@/api/library/library";
import type {
  EntryChanges,
  LibraryEntryDetail,
  NewPlays,
  Play,
  RunDate,
  Status,
  TitleRef,
  WatchedEpisode,
} from "@/api/schemas";
import { reportFailure } from "@/components/ui/error-state";
import { toast } from "@/components/ui/toast";
import { formatRunDate } from "@/components/ui/when-field";
import {
  addToLibrary,
  currentRun,
  invalidateTracking,
} from "@/features/library/entry";
import { today } from "@/lib/format";
import { type EpisodeRef, episodeKey, episodeLabel } from "./episodes";

type Entry = LibraryEntryDetail | null | undefined;

interface Snapshot {
  previous: Entry;
}

/** The title page's cached entry for `ref`, and its optimistic updates. */
export function useEntryCache(ref: TitleRef) {
  const queryClient = useQueryClient();
  const queryKey = getGetLibraryEntryQueryKey(ref);
  return {
    queryClient,
    /** Applies `update` to the cached entry and returns what it replaced. */
    async optimistic(
      update: (entry: LibraryEntryDetail) => LibraryEntryDetail,
    ): Promise<Snapshot> {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<Entry>(queryKey);
      if (previous) queryClient.setQueryData(queryKey, update(previous));
      return { previous };
    },
    /** The cached entry as it stands. */
    current(): Entry {
      return queryClient.getQueryData<Entry>(queryKey);
    },
    rollback(snapshot: Snapshot | undefined): void {
      if (snapshot) queryClient.setQueryData(queryKey, snapshot.previous);
    },
    settle(): Promise<void> {
      return invalidateTracking(queryClient, ref);
    },
  };
}

/** Only the members a change sets; `null` (clear) counts as set. */
function present(changes: EntryChanges): Partial<LibraryEntryDetail> {
  return Object.fromEntries(
    Object.entries(changes).filter(([, value]) => value !== undefined),
  );
}

/** `PATCH /library/{ref}`: merge semantics, so send only what changed. */
export function useUpdateEntry(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: (changes: EntryChanges) => updateLibraryEntry(ref, changes),
    onMutate: (changes) =>
      cache.optimistic((entry) => ({ ...entry, ...present(changes) })),
    onError: (error, _changes, snapshot) => {
      cache.rollback(snapshot);
      reportFailure(error);
    },
    onSettled: () => cache.settle(),
  });
}

/** Adds the title to the library with `status`. */
export function useAddEntry(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: (status: Status) => addToLibrary(ref, status),
    onSuccess: (outcome, status) => {
      toast(
        outcome === "kept"
          ? { title: "Already in your library" }
          : { tone: "success", title: `Added to ${statusWord(status)}` },
      );
    },
    onError: reportFailure,
    onSettled: () => cache.settle(),
  });
}

function statusWord(status: Status): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}

export interface LogRequest {
  plays: NewPlays;
  /** The episodes to show watched before the server answers. */
  episodes: readonly EpisodeRef[];
  /** The toast's words, e.g. "Logged S2E1". */
  message: string;
}

/** Logs plays, with an undo that voids exactly the plays it logged. */
export function useLogPlays(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: ({ plays }: LogRequest) => logPlays(ref, plays),
    onMutate: ({ episodes }) =>
      cache.optimistic((entry) => {
        const have = new Set(
          entry.watched.map((w) => episodeKey(w.season, w.episode)),
        );
        // Not yet logged, so no play ids until the refetch brings them.
        const added = episodes
          .filter((e) => !have.has(episodeKey(e.season, e.episode)))
          .map((e) => ({
            season: e.season,
            episode: e.episode,
            play_ids: [],
          }));
        return { ...entry, watched: [...entry.watched, ...added] };
      }),
    onSuccess: ({ plays }, { message }) => {
      if (plays.length === 0) {
        toast({ title: "Already watched", description: message });
        return;
      }
      toastLogged(plays, message, () => cache.settle());
    },
    onError: (error, _request, snapshot) => {
      cache.rollback(snapshot);
      reportFailure(error);
    },
    onSettled: () => cache.settle(),
  });
}

/** Says `plays` were logged, with an undo that voids exactly them. */
export function toastLogged(
  plays: readonly Play[],
  message: string,
  settle: () => Promise<void>,
  description?: string,
): void {
  toast({
    tone: "success",
    title: message,
    description,
    action: {
      label: "Undo",
      onClick: () => {
        void Promise.all(plays.map((play) => voidPlay(play.id)))
          .catch(reportFailure)
          .finally(settle);
      },
    },
  });
}

/**
 * Un-watches an episode by voiding the live plays the entry names for it
 * (`WatchedEpisode.play_ids`); the toast's undo restores exactly those.
 */
export function useUnwatchEpisode(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: async (target: WatchedEpisode): Promise<string[]> => {
      if (target.play_ids.length === 0) {
        // Shown watched before its plays were logged; nothing to void yet.
        throw new Error(`${episodeLabel(target)} is still being logged.`);
      }
      await Promise.all(target.play_ids.map((id) => voidPlay(id)));
      return target.play_ids;
    },
    onMutate: (target) =>
      cache.optimistic((entry) => ({
        ...entry,
        watched: entry.watched.filter(
          (w) => w.season !== target.season || w.episode !== target.episode,
        ),
      })),
    onSuccess: (plays, target) => {
      toast({
        title: `Marked ${episodeLabel(target)} unwatched`,
        action: {
          label: "Undo",
          onClick: () => {
            void Promise.all(plays.map((id) => restorePlay(id)))
              .catch(reportFailure)
              .finally(() => cache.settle());
          },
        },
      });
    },
    onError: (error, _target, snapshot) => {
      cache.rollback(snapshot);
      reportFailure(error);
    },
    onSettled: () => cache.settle(),
  });
}

/**
 * Starts a rewatch: a new run, with its progress starting from nothing. It
 * starts on `started`, or today when that is `null`.
 */
export function useStartRewatch(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: (started: RunDate | null) =>
      startRewatch(ref, started ? { started } : {}),
    onMutate: (started) =>
      cache.optimistic((entry) => ({
        ...entry,
        status: "watching",
        watched: [],
        runs: [
          ...entry.runs,
          {
            number: currentRun(entry.runs) + 1,
            started: started ?? { date: today(), precision: "day" },
            completed: null,
          },
        ],
      })),
    onSuccess: (run, started) => {
      toast({
        tone: "success",
        title: `Rewatch started (run ${run.number})`,
        description: started ? `Started ${formatRunDate(started)}` : undefined,
      });
    },
    onError: (error, _vars, snapshot) => {
      cache.rollback(snapshot);
      reportFailure(error);
    },
    onSettled: () => cache.settle(),
  });
}
