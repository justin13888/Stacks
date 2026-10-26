import { useQueryClient } from "@tanstack/react-query";
import { Flag, Play, Plus, RotateCcw } from "lucide-react";
import { useState } from "react";
import type {
  LibraryEntryDetail,
  Season,
  TitleDetail,
  TitleRef,
  WatchRun,
} from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { reportFailure } from "@/components/ui/error-state";
import { ProgressRing } from "@/components/ui/progress-ring";
import { toast } from "@/components/ui/toast";
import { formatRunDate } from "@/components/ui/when-field";
import { currentRun, resolveNextEpisode } from "@/features/library/entry";
import { episodeLabel } from "./episodes";
import { useFinishRun, useMarkWatchedOn } from "./use-run-dates";
import { useLogPlays, useStartRewatch } from "./use-tracking";
import { DatedAction, WhenDialog } from "./when-dialog";

/**
 * Progress in the current run, "+1 episode" (or "mark watched"), and
 * rewatch. Marking a movie watched and starting a rewatch each happen now,
 * or on a date picked from the calendar button beside them.
 */
export function Progress({
  title,
  entry,
  seasons,
}: {
  title: TitleDetail;
  entry: LibraryEntryDetail | null;
  seasons: readonly Season[] | undefined;
}) {
  const queryClient = useQueryClient();
  const log = useLogPlays(title.ref);
  const markOn = useMarkWatchedOn(title.ref);
  const rewatch = useStartRewatch(title.ref);
  const [resolving, setResolving] = useState(false);
  const isShow = title.kind === "show";
  const run = entry ? currentRun(entry.runs) : 1;
  const watchedCount = isShow
    ? (entry?.watched.filter((w) => w.season > 0).length ?? 0)
    : (entry?.progress.current_run_plays ?? 0);

  async function plusOne() {
    setResolving(true);
    try {
      const next = await resolveNextEpisode(queryClient, title.ref);
      if (!next) {
        reportNothingLeft();
        return;
      }
      log.mutate({
        plays: { kind: "episodes", episodes: [next] },
        episodes: [next],
        message: `Logged ${episodeLabel(next)}`,
      });
    } catch (error) {
      reportFailure(error);
    } finally {
      setResolving(false);
    }
  }

  const markLabel = watchedCount > 0 ? "Watched again" : "Mark watched";
  return (
    <div className="flex flex-wrap items-center gap-4">
      {isShow ? (
        <ProgressRing
          value={watchedCount}
          max={title.episode_count}
          size={56}
          label={`Episodes watched in run ${run}`}
        />
      ) : null}
      <div className="flex flex-col">
        <span className="text-sm font-semibold text-fg">
          {isShow
            ? `${watchedCount}${title.episode_count ? ` of ${title.episode_count}` : ""} episodes`
            : watchedCount > 0
              ? "Watched"
              : "Not watched yet"}
        </span>
        {entry && entry.runs.length > 1 && (
          <span className="text-xs text-fg-muted">
            Run {run} · rewatch {run - 1}
          </span>
        )}
      </div>
      <div className="ml-auto flex flex-wrap gap-2">
        {isShow ? (
          <Button
            onClick={() => void plusOne()}
            loading={resolving || log.isPending}
            disabled={!seasons}
          >
            <Plus aria-hidden className="size-4" />
            +1 episode
          </Button>
        ) : (
          <DatedAction
            label={`${markLabel} on a date…`}
            dialog={{
              title: "When did you watch it?",
              description:
                "Pick the day, or just the month or year if that's all you remember.",
              label: "Watched",
              confirmLabel: "Mark watched",
            }}
            pending={markOn.isPending}
            onConfirm={(date) => markOn.mutateAsync(date)}
          >
            <Button
              onClick={() =>
                log.mutate({
                  plays: { kind: "movie" },
                  episodes: [],
                  message: "Marked as watched",
                })
              }
              loading={log.isPending}
            >
              <Play aria-hidden className="size-4" />
              {markLabel}
            </Button>
          </DatedAction>
        )}
        {entry && (
          <DatedAction
            label="Start a rewatch on a date…"
            dialog={{
              title: "When did the rewatch start?",
              description:
                "Pick the day, or just the month or year if that's all you remember.",
              label: "Started",
              confirmLabel: "Start rewatch",
            }}
            pending={rewatch.isPending}
            onConfirm={(date) => rewatch.mutateAsync(date)}
          >
            <Button
              variant="secondary"
              onClick={() => rewatch.mutate(null)}
              loading={rewatch.isPending}
            >
              <RotateCcw aria-hidden className="size-4" />
              Start a rewatch
            </Button>
          </DatedAction>
        )}
      </div>
    </div>
  );
}

function reportNothingLeft() {
  toast({
    title: "You're all caught up",
    description: "Every episode listed so far is watched.",
  });
}

/**
 * Every watch-through: the first watch, then each rewatch. A run not yet
 * finished can be finished on a date; finishing the current one completes
 * the title.
 */
export function RunHistory({
  titleRef,
  runs,
}: {
  titleRef: TitleRef;
  runs: readonly WatchRun[];
}) {
  const latest = currentRun(runs);
  const finish = useFinishRun(titleRef);
  const [finishing, setFinishing] = useState<number | null>(null);
  return (
    <section aria-labelledby="runs-heading" className="flex flex-col gap-2">
      <h2 id="runs-heading" className="text-sm font-semibold text-fg">
        Watch history
      </h2>
      <ol className="flex flex-col gap-1">
        {runs.map((run) => {
          const started = formatRunDate(run.started);
          const completed = formatRunDate(run.completed);
          return (
            <li
              key={run.number}
              className="flex min-h-8 flex-wrap items-center gap-x-2 text-sm text-fg-muted"
            >
              <span className="font-medium text-fg">
                {run.number === 1 ? "First watch" : `Rewatch ${run.number - 1}`}
              </span>
              <span>·</span>
              <span>Run {run.number}</span>
              {started && <span>· started {started}</span>}
              <span>
                ·{" "}
                {completed
                  ? `completed ${completed}`
                  : run.number === latest
                    ? "in progress"
                    : "not finished"}
              </span>
              {!run.completed && (
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={`Finish run ${run.number}…`}
                  onClick={() => setFinishing(run.number)}
                  className="h-7 px-2"
                >
                  <Flag aria-hidden className="size-3.5" />
                  Finish…
                </Button>
              )}
            </li>
          );
        })}
      </ol>
      <WhenDialog
        open={finishing !== null}
        onOpenChange={(open) => {
          if (!open) setFinishing(null);
        }}
        title={`Finish run ${finishing ?? ""}`}
        description={
          finishing === latest
            ? "When did you finish? The title moves to Completed."
            : "When did you finish this run?"
        }
        label="Finished"
        confirmLabel="Mark finished"
        pending={finish.isPending}
        onConfirm={(date) => {
          if (finishing === null) return;
          finish.mutate(
            { run: finishing, date },
            { onSuccess: () => setFinishing(null) },
          );
        }}
      />
    </section>
  );
}
