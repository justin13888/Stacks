import { Link } from "@tanstack/react-router";
import { CalendarX2, Clock, Film, Trophy } from "lucide-react";
import type { ReactNode } from "react";
import { useListLibrary } from "@/api/library/library";
import type { Stats } from "@/api/schemas";
import { useGetStats } from "@/api/stats/stats";
import { PageHeader } from "@/components/layout/page";
import { CATEGORY_LABELS } from "@/components/search/categories";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Poster } from "@/components/ui/poster";
import { Rating } from "@/components/ui/rating";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { humanizeMinutes } from "@/lib/format";
import { HorizontalBars, MonthHeatmap, monthsOf } from "./charts";
import { CountUp } from "./count-up";

/** How many genres the breakdown shows. */
const TOP_GENRES = 10;
/** How many titles "Highest rated" shows. */
const TOP_RATED = 5;

export interface StatsSearch {
  year?: number;
}

export function validateStatsSearch(raw: Record<string, unknown>): StatsSearch {
  const year = Number(raw.year);
  return {
    year:
      Number.isInteger(year) && year >= 1900 && year <= 3000 ? year : undefined,
  };
}

/** Every year with activity, and the current one, newest first. */
export function yearsOf(allTime: Stats | undefined, current: number): number[] {
  const years = new Set<number>([current]);
  for (const { year } of allTime?.completions_by_year ?? []) years.add(year);
  for (const { month } of allTime?.monthly_activity ?? []) {
    const year = Number(month.slice(0, 4));
    if (Number.isInteger(year)) years.add(year);
  }
  return [...years].sort((a, b) => b - a);
}

function Section({
  id,
  title,
  description,
  children,
}: {
  id: string;
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section
      aria-labelledby={id}
      className="flex flex-col gap-3 rounded-xl border border-line bg-surface p-4 md:p-6"
    >
      <div>
        <h2 id={id} className="font-display text-lg font-semibold text-fg">
          {title}
        </h2>
        {description && <p className="text-sm text-fg-muted">{description}</p>}
      </div>
      {children}
    </section>
  );
}

function Tile({
  icon,
  label,
  children,
}: {
  icon: ReactNode;
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1 rounded-xl border border-line bg-surface p-4 shadow-raised">
      <span className="flex items-center gap-1.5 text-sm text-fg-muted [&_svg]:size-4 [&_svg]:text-accent">
        {icon}
        {label}
      </span>
      <span className="font-display text-3xl font-bold tracking-tight text-fg tabular-nums">
        {children}
      </span>
    </div>
  );
}

/** A year in review: what was watched, when, and what it was. */
export function StatsPage({
  search,
  onYearChange,
}: {
  search: StatsSearch;
  onYearChange: (year: number) => void;
}) {
  const currentYear = new Date().getFullYear();
  const year = search.year ?? currentYear;
  const allTime = useGetStats();
  const stats = useGetStats({ year });
  const years = yearsOf(allTime.data, currentYear);
  if (!years.includes(year)) years.push(year);
  years.sort((a, b) => b - a);

  let body: ReactNode;
  if (stats.isPending) {
    body = (
      <div aria-hidden className="flex flex-col gap-4">
        <Skeleton className="h-28" />
        <Skeleton className="h-40" />
        <Skeleton className="h-64" />
      </div>
    );
  } else if (!stats.data) {
    body = (
      <ErrorState error={stats.error} onRetry={() => void stats.refetch()} />
    );
  } else {
    body = <YearInReview stats={stats.data} year={year} />;
  }

  return (
    <>
      <PageHeader
        title="Stats"
        description="Your year in review: what you watched, when, and what it was."
        actions={
          <Select<string>
            label="Year"
            value={String(year)}
            options={years.map((y) => ({ value: String(y), label: String(y) }))}
            onValueChange={(value) => onYearChange(Number(value))}
            className="w-32"
          />
        }
      />
      {body}
    </>
  );
}

function YearInReview({ stats, year }: { stats: Stats; year: number }) {
  const plays = stats.monthly_activity.reduce((n, m) => n + m.plays, 0);
  const completed =
    stats.completions_by_year.find((c) => c.year === year)?.completed ?? 0;
  const empty =
    plays === 0 && completed === 0 && stats.watch_time_minutes === 0;

  return (
    <div className="flex flex-col gap-6">
      {empty ? (
        <EmptyState
          icon={<CalendarX2 />}
          title={`Nothing logged in ${year}`}
          description="Log an episode or finish a title, and this year's review fills in."
        />
      ) : (
        <>
          <section
            aria-label={`${year} totals`}
            className="grid gap-3 sm:grid-cols-3"
          >
            <Tile icon={<Clock aria-hidden />} label="Watch time">
              <CountUp
                value={stats.watch_time_minutes}
                format={humanizeMinutes}
              />
            </Tile>
            <Tile icon={<Film aria-hidden />} label="Episodes and films">
              <CountUp value={plays} />
            </Tile>
            <Tile icon={<Trophy aria-hidden />} label="Titles completed">
              <CountUp value={completed} />
            </Tile>
          </section>
          <Section
            id="stats-activity"
            title="Monthly activity"
            description="Episodes and films logged each month."
          >
            <MonthHeatmap
              months={monthsOf(year, stats.monthly_activity)}
              label={`Plays per month in ${year}`}
            />
          </Section>
          <Section
            id="stats-genres"
            title="Top genres"
            description={`The genres of what you completed in ${year}.`}
          >
            {stats.top_genres.length === 0 ? (
              <p className="text-sm text-fg-muted">
                Nothing completed in {year} yet.
              </p>
            ) : (
              <HorizontalBars
                label={`Top genres in ${year}`}
                unit="titles"
                data={stats.top_genres
                  .slice(0, TOP_GENRES)
                  .map((g) => ({ label: g.genre, value: g.count }))}
              />
            )}
          </Section>
        </>
      )}
      <div className="grid gap-6 lg:grid-cols-2">
        <Section
          id="stats-categories"
          title="Your library by category"
          description="Everything you track, as it stands today."
        >
          <HorizontalBars
            label="Library titles per category"
            unit="titles"
            data={stats.by_category
              .slice()
              .sort((a, b) => b.count - a.count)
              .map((c) => ({
                label: CATEGORY_LABELS[c.category],
                value: c.count,
              }))}
          />
        </Section>
        <Section
          id="stats-statuses"
          title="Your library by status"
          description="Where each title stands today."
        >
          <HorizontalBars
            label="Library titles per status"
            unit="titles"
            data={stats.by_status
              .slice()
              .sort((a, b) => b.count - a.count)
              .map((s) => ({ label: STATUS_LABELS[s.status], value: s.count }))}
          />
        </Section>
      </div>
      <HighestRated />
    </div>
  );
}

/** The best-scored completed titles in the library. */
function HighestRated() {
  const { title_language, score_format } = usePreferences();
  const top = useListLibrary({
    status: "completed",
    sort: "score",
    limit: TOP_RATED,
  });
  const items = (top.data?.items ?? []).filter((e) => e.score != null);
  if (items.length === 0) return null;
  return (
    <Section
      id="stats-top-rated"
      title="Highest rated"
      description="Your best-scored completed titles."
    >
      <ol className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-5">
        {items.map((entry, index) => {
          const name = displayTitle(entry.title, title_language);
          return (
            <li key={entry.title.ref} className="flex flex-col gap-1.5">
              <Link
                to="/titles/$ref"
                params={{ ref: entry.title.ref }}
                className="flex flex-col gap-1.5 rounded-poster focus-visible:outline-2 focus-visible:outline-ring"
              >
                <Poster
                  src={entry.title.poster_url}
                  title={name}
                  className="w-full"
                />
                <span className="line-clamp-2 text-sm font-medium text-fg">
                  <span className="text-fg-subtle">{index + 1}. </span>
                  {name}
                </span>
              </Link>
              <Rating
                score={entry.score}
                format={score_format}
                className="text-xs"
              />
            </li>
          );
        })}
      </ol>
    </Section>
  );
}
