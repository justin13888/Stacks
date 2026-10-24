// The title page's writes. Each one updates the cached entry at once
// (optimistically), puts it back if the server refuses, and then refetches
// everything the change can alter.

import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  getGetLibraryEntryQueryKey,
  logPlays,
  startRewatch,
  updateLibraryEntry,
  voidPlay,
} from "@/api/library/library";
import type {
  EntryChanges,
  LibraryEntryDetail,
  NewPlays,
  Play,
  Status,
  TitleRef,
} from "@/api/schemas";
import { describeError } from "@/components/ui/error-state";
import { toast } from "@/components/ui/toast";
import {
  addToLibrary,
  currentRun,
  findLivePlays,
  invalidateTracking,
} from "@/features/library/entry";
import { isUnauthenticated } from "@/hooks/use-auth";
import { today } from "@/features/calendar/dates";
import { type EpisodeRef, episodeKey, episodeLabel } from "./episodes";

type Entry = LibraryEntryDetail | null | undefined;

interface Snapshot {
  previous: Entry;
}

/** A failed write, unless the session ended (the shell handles that). */
export function reportFailure(error: unknown): void {
  if (isUnauthenticated(error)) return;
  const { title, detail } = describeError(error);
  toast({ tone: "error", title, description: detail });
}

function useEntryCache(ref: TitleRef) {
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
    onSuccess: (_entry, status) => {
      toast({ tone: "success", title: `Added to ${statusWord(status)}` });
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
        const added = episodes
          .filter((e) => !have.has(episodeKey(e.season, e.episode)))
          .map((e) => ({ season: e.season, episode: e.episode }));
        return { ...entry, watched: [...entry.watched, ...added] };
      }),
    onSuccess: ({ plays }, { message }) => {
      if (plays.length === 0) {
        toast({ title: "Already watched", description: message });
        return;
      }
      toast({
        tone: "success",
        title: message,
        action: {
          label: "Undo",
          onClick: () => {
            void Promise.all(plays.map((play) => voidPlay(play.id)))
              .catch(reportFailure)
              .finally(() => cache.settle());
          },
        },
      });
    },
    onError: (error, _request, snapshot) => {
      cache.rollback(snapshot);
      reportFailure(error);
    },
    onSettled: () => cache.settle(),
  });
}

/** Un-watches an episode by voiding its plays; the toast's undo logs it again. */
export function useUnwatchEpisode(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: async (target: EpisodeRef): Promise<Play[]> => {
      const entry = cache.queryClient.getQueryData<Entry>(
        getGetLibraryEntryQueryKey(ref),
      );
      const plays = await findLivePlays(
        ref,
        currentRun(entry?.runs ?? []),
        target,
      );
      if (plays.length === 0) {
        throw new Error(`No play of ${episodeLabel(target)} could be found.`);
      }
      await Promise.all(plays.map((play) => voidPlay(play.id)));
      return plays;
    },
    onMutate: (target) =>
      cache.optimistic((entry) => ({
        ...entry,
        watched: entry.watched.filter(
          (w) => w.season !== target.season || w.episode !== target.episode,
        ),
      })),
    onSuccess: (plays, target) => {
      const watchedAt = plays[0]?.watched_at ?? undefined;
      toast({
        title: `Marked ${episodeLabel(target)} unwatched`,
        action: {
          label: "Undo",
          onClick: () => {
            void logPlays(ref, {
              kind: "episodes",
              episodes: [{ season: target.season, episode: target.episode }],
              watched_at: watchedAt,
            })
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

/** Starts a rewatch: a new run, with its progress starting from nothing. */
export function useStartRewatch(ref: TitleRef) {
  const cache = useEntryCache(ref);
  return useMutation({
    mutationFn: () => startRewatch(ref, {}),
    onMutate: () =>
      cache.optimistic((entry) => ({
        ...entry,
        status: "watching",
        watched: [],
        runs: [
          ...entry.runs,
          {
            number: currentRun(entry.runs) + 1,
            started: { date: today(), precision: "day" },
            completed: null,
          },
        ],
      })),
    onSuccess: (run) => {
      toast({ tone: "success", title: `Rewatch started (run ${run.number})` });
    },
    onError: (error, _vars, snapshot) => {
      cache.rollback(snapshot);
      reportFailure(error);
    },
    onSettled: () => cache.settle(),
  });
}
