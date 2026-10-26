import { useQuery } from "@tanstack/react-query";
import { CloudOff } from "lucide-react";
import {
  useGetTitle,
  useListRecommendations,
  useListSeasons,
} from "@/api/catalog/catalog";
import type {
  Format,
  LibraryEntryDetail,
  TitleDetail,
  TitleRef,
} from "@/api/schemas";
import { CATEGORY_NAMES } from "@/components/search/categories";
import { ErrorState } from "@/components/ui/error-state";
import { Poster } from "@/components/ui/poster";
import { Skeleton } from "@/components/ui/skeleton";
import { StatusBadge } from "@/components/ui/status-badge";
import {
  useHiddenRefs,
  withoutHidden,
} from "@/features/blocklist/use-blocklist";
import { TitleCard } from "@/features/discover/title-card";
import { TitleHistory } from "@/features/history/history-page";
import { entryQueryOptions } from "@/features/library/entry";
import { formatInstant } from "@/lib/format";
import { LibraryControls } from "./library-controls";
import { displayTitle, usePreferences } from "./preferences";
import { Progress, RunHistory } from "./progress";
import { SeasonsGrid } from "./seasons-grid";
import { TitleActions } from "./title-actions";

/** The title page: who it is, where you are with it, and its episodes. */
export function TitlePage({ titleRef }: { titleRef: TitleRef }) {
  const preferences = usePreferences();
  const title = useGetTitle(titleRef);
  const entry = useQuery(entryQueryOptions(titleRef));
  const isShow = title.data?.kind === "show";
  const seasons = useListSeasons(titleRef, { query: { enabled: isShow } });

  if (title.isPending) return <TitleSkeleton />;
  if (title.isError) {
    return (
      <ErrorState error={title.error} onRetry={() => void title.refetch()} />
    );
  }

  const name = displayTitle(title.data, preferences.title_language);
  const entryData = entry.data ?? null;
  return (
    <article className="flex flex-col gap-8">
      <Hero title={title.data} name={name} entry={entryData} />
      {entry.isError ? (
        <ErrorState error={entry.error} onRetry={() => void entry.refetch()} />
      ) : entry.isPending ? (
        <Skeleton className="h-24" />
      ) : (
        <div className="flex flex-col gap-6 rounded-xl border border-line bg-surface p-4 md:p-6">
          <Progress
            title={title.data}
            entry={entryData}
            seasons={seasons.data?.items}
          />
          <LibraryControls
            titleRef={titleRef}
            entry={entryData}
            scoreFormat={preferences.score_format}
          />
          <TitleActions title={title.data} name={name} entry={entryData} />
          {entryData && entryData.runs.length > 0 && (
            <RunHistory titleRef={titleRef} runs={entryData.runs} />
          )}
        </div>
      )}
      {isShow &&
        (seasons.isPending ? (
          <Skeleton className="h-48" />
        ) : seasons.isError ? (
          <ErrorState
            error={seasons.error}
            onRetry={() => void seasons.refetch()}
          />
        ) : (
          <SeasonsGrid
            titleRef={titleRef}
            seasons={seasons.data.items}
            entry={entryData}
          />
        ))}
      {entryData && <TitleHistory title={title.data} />}
      <Recommendations titleRef={titleRef} />
    </article>
  );
}

function TitleSkeleton() {
  return (
    <div role="status" aria-label="Loading title" className="flex gap-6">
      <Skeleton className="aspect-[2/3] w-36 md:w-56" />
      <div className="flex flex-1 flex-col gap-3">
        <Skeleton className="h-8 w-2/3" />
        <Skeleton className="h-4 w-1/3" />
        <Skeleton className="h-20 w-full" />
      </div>
    </div>
  );
}

function runtime(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours} h ${rest} min` : `${hours} h`;
}

const FORMAT_NAMES: Record<Format, string> = {
  tv: "TV",
  tv_short: "TV short",
  movie: "Movie",
  ova: "OVA",
  ona: "ONA",
  special: "Special",
  miniseries: "Miniseries",
};

/** The release format, unless the category already says it ("Movie"). */
function formatFact(title: TitleDetail): string | null {
  if (!title.format) return null;
  const name = FORMAT_NAMES[title.format];
  return name === CATEGORY_NAMES[title.category] ? null : name;
}

function Hero({
  title,
  name,
  entry,
}: {
  title: TitleDetail;
  name: string;
  entry: LibraryEntryDetail | null;
}) {
  const facts = [
    title.year,
    CATEGORY_NAMES[title.category],
    formatFact(title),
    title.kind === "show" && title.season_count
      ? `${title.season_count} season${title.season_count === 1 ? "" : "s"}`
      : null,
    title.runtime_minutes
      ? `${runtime(title.runtime_minutes)}${title.kind === "show" ? " / ep" : ""}`
      : null,
    title.release_status,
  ].filter(Boolean);
  const subtitle =
    title.original_title && title.original_title !== name
      ? title.original_title
      : title.title !== name
        ? title.title
        : null;
  return (
    <header className="relative isolate overflow-hidden rounded-xl border border-line bg-surface">
      {title.backdrop_url && (
        <img
          src={title.backdrop_url}
          alt=""
          aria-hidden
          className="absolute inset-0 -z-10 size-full object-cover opacity-30"
        />
      )}
      <div
        aria-hidden
        className="absolute inset-0 -z-10 bg-gradient-to-t from-surface via-surface/80 to-surface/30"
      />
      <div className="flex flex-col gap-5 p-4 sm:flex-row md:p-6">
        <Poster
          src={title.poster_url}
          title={name}
          size="lg"
          priority
          className="w-36 self-center sm:w-44 sm:self-start md:w-56"
        />
        <div className="flex min-w-0 flex-1 flex-col gap-3">
          <div className="flex flex-wrap items-center gap-2">
            {entry && (
              <StatusBadge
                status={entry.status}
                rewatching={title.library?.rewatching ?? false}
              />
            )}
            {title.freshness.stale && (
              <span
                role="status"
                className="inline-flex items-center gap-1.5 rounded-full bg-warning/12 px-2 py-0.5 text-xs font-medium text-fg ring-1 ring-warning/30 ring-inset"
                title={
                  title.freshness.fetched_at
                    ? `Saved ${formatInstant(title.freshness.fetched_at)}`
                    : undefined
                }
              >
                <CloudOff aria-hidden className="size-3.5 text-warning" />
                Saved copy — may be out of date
              </span>
            )}
          </div>
          <h1 className="font-display text-2xl font-bold tracking-tight text-fg text-balance md:text-4xl">
            {name}
          </h1>
          {subtitle && <p className="text-sm text-fg-muted">{subtitle}</p>}
          {facts.length > 0 && (
            <p className="text-sm text-fg-muted">{facts.join(" · ")}</p>
          )}
          {title.genres.length > 0 && (
            <ul aria-label="Genres" className="flex flex-wrap gap-1.5">
              {title.genres.map((genre) => (
                <li
                  key={genre}
                  className="rounded-full bg-surface-sunken px-2.5 py-0.5 text-xs text-fg-muted"
                >
                  {genre}
                </li>
              ))}
            </ul>
          )}
          {title.tagline && (
            <p className="text-sm italic text-fg-muted">{title.tagline}</p>
          )}
          {title.overview && (
            <p className="max-w-prose text-sm leading-relaxed text-fg">
              {title.overview}
            </p>
          )}
        </div>
      </div>
    </header>
  );
}

/**
 * Titles like this one. A provider without recommendations (AniList) says
 * so instead of leaving a gap; a failure hides the rail rather than the page.
 */
function Recommendations({ titleRef }: { titleRef: TitleRef }) {
  const recommendations = useListRecommendations(titleRef);
  const hidden = useHiddenRefs();
  if (recommendations.isPending || recommendations.isError) return null;
  const { items, degraded } = recommendations.data;
  const visible = withoutHidden(items, hidden);
  const unsupported = degraded.some((d) => d.class === "unsupported");
  if (visible.length === 0 && !unsupported) return null;
  return (
    <section aria-labelledby="recs-heading" className="flex flex-col gap-3">
      <h2
        id="recs-heading"
        className="font-display text-lg font-semibold text-fg"
      >
        More like this
      </h2>
      {visible.length === 0 ? (
        <p className="text-sm text-fg-muted">
          Recommendations aren't available for this title's provider.
        </p>
      ) : (
        <ul className="-mx-1 flex gap-3 overflow-x-auto px-1 pb-2">
          {visible.map((item) => (
            <li key={item.ref} className="shrink-0">
              <TitleCard title={item} size="sm" />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
