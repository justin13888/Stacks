import { useQueryClient } from "@tanstack/react-query";
import { Check, ListChecks } from "lucide-react";
import { useState } from "react";
import { getGetSeasonQueryKey, useGetSeason } from "@/api/catalog/catalog";
import type {
  Episode,
  LibraryEntryDetail,
  Season,
  SeasonDetail,
  TitleRef,
} from "@/api/schemas";
import { IconButton } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { formatDay, today } from "@/features/calendar/dates";
import { cn } from "@/lib/cn";
import {
  type EpisodeRef,
  episodeKey,
  episodeLabel,
  markThroughPlays,
  orderSeasons,
  throughEpisodes,
  watchedKeys,
} from "./episodes";
import { useLogPlays, useUnwatchEpisode } from "./use-tracking";

function seasonName(season: Season): string {
  if (season.number === 0) return "Specials";
  return season.name?.trim() || `Season ${season.number}`;
}

/**
 * Season tabs (specials last) over a grid of episode tiles. A tile toggles
 * its episode watched or unwatched; its second button marks every episode up
 * to it watched in one request.
 */
export function SeasonsGrid({
  titleRef,
  seasons,
  entry,
}: {
  titleRef: TitleRef;
  seasons: readonly Season[];
  entry: LibraryEntryDetail | null;
}) {
  const ordered = orderSeasons(seasons);
  const first = ordered.find((s) => s.number > 0) ?? ordered[0];
  const [tab, setTab] = useState(String(first?.number ?? 1));

  if (ordered.length === 0) {
    return (
      <EmptyState
        title="No episodes listed yet"
        description="The provider hasn't published this show's seasons."
      />
    );
  }

  return (
    <section aria-labelledby="seasons-heading" className="flex flex-col gap-3">
      <h2
        id="seasons-heading"
        className="font-display text-lg font-semibold text-fg"
      >
        Episodes
      </h2>
      <Tabs value={tab} onValueChange={setTab}>
        <div className="-mx-1 overflow-x-auto px-1 pb-1">
          <TabsList aria-label="Seasons">
            {ordered.map((season) => (
              <TabsTrigger key={season.number} value={String(season.number)}>
                {seasonName(season)}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        {ordered.map((season) => (
          <TabsContent key={season.number} value={String(season.number)}>
            <SeasonEpisodes
              titleRef={titleRef}
              season={season}
              seasons={seasons}
              entry={entry}
            />
          </TabsContent>
        ))}
      </Tabs>
    </section>
  );
}

function SeasonEpisodes({
  titleRef,
  season,
  seasons,
  entry,
}: {
  titleRef: TitleRef;
  season: Season;
  seasons: readonly Season[];
  entry: LibraryEntryDetail | null;
}) {
  const queryClient = useQueryClient();
  const detail = useGetSeason(titleRef, season.number);
  const log = useLogPlays(titleRef);
  const unwatch = useUnwatchEpisode(titleRef);
  const watched = watchedKeys(entry?.watched ?? []);
  const busy = log.isPending || unwatch.isPending;

  if (detail.isPending) {
    return (
      <ul aria-hidden className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {Array.from({ length: 6 }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
          <Skeleton key={i} className="h-16" />
        ))}
      </ul>
    );
  }
  if (detail.isError) {
    return (
      <ErrorState error={detail.error} onRetry={() => void detail.refetch()} />
    );
  }
  if (detail.data.episodes.length === 0) {
    return <EmptyState title="No episodes listed for this season yet" />;
  }

  function loaded(number: number): readonly Episode[] | undefined {
    return queryClient.getQueryData<SeasonDetail>(
      getGetSeasonQueryKey(titleRef, number),
    )?.episodes;
  }

  function toggle(target: EpisodeRef) {
    const seen = entry?.watched.find(
      (w) => w.season === target.season && w.episode === target.episode,
    );
    if (seen) {
      unwatch.mutate(seen);
    } else {
      log.mutate({
        plays: {
          kind: "episodes",
          episodes: [{ season: target.season, episode: target.episode }],
        },
        episodes: [target],
        message: `Logged ${episodeLabel(target)}`,
      });
    }
  }

  function markThrough(target: EpisodeRef) {
    const plays = markThroughPlays(
      target,
      detail.data?.episodes ?? [],
      watched,
    );
    if (!plays) return;
    log.mutate({
      plays,
      episodes:
        plays.kind === "episodes"
          ? plays.episodes
          : throughEpisodes(target, seasons, loaded),
      message: `Marked up to ${episodeLabel(target)} watched`,
    });
  }

  const now = today();
  return (
    <ul className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
      {detail.data.episodes.map((episode) => {
        const target = { season: season.number, episode: episode.number };
        const isWatched = watched.has(
          episodeKey(target.season, target.episode),
        );
        const upcoming = episode.air_date ? episode.air_date > now : false;
        const label = episodeLabel(target);
        // A show numbered across all of it (a long-running anime) shows
        // that number: "episode 1,071" rather than its place in a season.
        const shown =
          episode.absolute_number != null
            ? episode.absolute_number.toLocaleString()
            : String(episode.number);
        const absolute =
          episode.absolute_number != null
            ? `Episode ${episode.absolute_number.toLocaleString()}`
            : null;
        return (
          <li
            key={episode.number}
            data-watched={isWatched || undefined}
            className={cn(
              "flex items-center gap-2 rounded-lg border border-line bg-surface p-2 transition-colors",
              isWatched && "border-accent/40 bg-accent-soft",
            )}
          >
            <button
              type="button"
              aria-pressed={isWatched}
              aria-label={`${label}${absolute ? ` (${absolute})` : ""}${episode.name ? `: ${episode.name}` : ""}`}
              disabled={busy}
              onClick={() => toggle(target)}
              className="flex min-w-0 flex-1 items-center gap-3 rounded-md text-left focus-visible:outline-2 focus-visible:outline-ring"
            >
              <span
                aria-hidden
                className={cn(
                  "flex h-10 min-w-10 shrink-0 items-center justify-center rounded-md px-1 text-sm font-semibold tabular-nums",
                  isWatched
                    ? "bg-accent text-fg-on-accent"
                    : "bg-surface-sunken text-fg",
                )}
              >
                {isWatched ? <Check className="size-5" /> : shown}
              </span>
              <span className="flex min-w-0 flex-col">
                <span className="truncate text-sm font-medium text-fg">
                  {episode.name || absolute || `Episode ${episode.number}`}
                </span>
                <span className="text-xs text-fg-muted">
                  {label}
                  {absolute && episode.name && ` · ${absolute}`}
                  {episode.air_date &&
                    ` · ${upcoming ? "Airs " : ""}${formatDay(episode.air_date, { month: "short", day: "numeric", year: "numeric" })}`}
                </span>
              </span>
            </button>
            <IconButton
              label={`Mark up to ${label} watched`}
              size="sm"
              icon={<ListChecks aria-hidden className="size-4" />}
              disabled={busy}
              onClick={() => markThrough(target)}
            />
          </li>
        );
      })}
    </ul>
  );
}
