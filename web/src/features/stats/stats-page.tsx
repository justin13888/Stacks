import { Link } from "@tanstack/react-router";
import {
  CalendarX2,
  Clock,
  Film,
  Flame,
  Popcorn,
  Trophy,
  Tv,
} from "lucide-react";
import type { ReactNode } from "react";
import type { Binge, RatedTitle, ScoreCount, Stats } from "@/api/schemas";
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
import { formatDay, humanizeMinutes } from "@/lib/format";
import {
  formatScore,
  maxScore,
  type ScoreFormat,
  toDisplayScore,
} from "@/lib/score-format";
import { HorizontalBars, MonthHeatmap, monthsOf } from "./charts";
import { CountUp } from "./count-up";

/** How many genres the breakdown shows. */
const TOP_GENRES = 10;
/** How many titles "Highest rated" shows, of the API's ten. */
const TOP_RATED = 5;
/** A day of the year in review, e.g. "Mar 2". */
const DAY: Intl.DateTimeFormatOptions = { month: "short", day: "numeric" };

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

/**
 * The score distribution in the user's format, highest first: one bar per
 * point of a 10-point (or finer) format, of a 5-point one, or per word of a
 * 3-point one, from the lowest it reaches to the top, so gaps show as gaps.
 */
export function scoreBuckets(
  distribution: readonly ScoreCount[],
  format: ScoreFormat,
): { label: string; value: number }[] {
  // Formats finer than ten points are binned to ten, or the chart would
  // carry up to a hundred bars.
  const binning: ScoreFormat =
    format === "point_100" || format === "point_10_decimal"
      ? "point_10"
      : format;
  const counts = new Map<number, number>();
  for (const { score, count } of distribution) {
    const bin = toDisplayScore(score, binning);
    counts.set(bin, (counts.get(bin) ?? 0) + count);
  }
  if (counts.size === 0) return [];
  const top = maxScore(binning);
  const step = 100 / top;
  const lowest = Math.min(...counts.keys());
  const buckets = [];
  for (let bin = top; bin >= lowest; bin -= 1) {
    buckets.push({
      label: formatScore(bin * step, format),
      value: counts.get(bin) ?? 0,
    });
  }
  return buckets;
}

/** "1 day", "12 days". */
function daysWord(days: number): string {
  return days === 1 ? "1 day" : `${days} days`;
}

function YearInReview({ stats, year }: { stats: Stats; year: number }) {
  const { score_format } = usePreferences();
  const plays = stats.episode_plays + stats.movie_plays;
  const completed =
    stats.completions_by_year.find((c) => c.year === year)?.completed ?? 0;
  const empty =
    plays === 0 && completed === 0 && stats.watch_time_minutes === 0;
  const scores = scoreBuckets(stats.score_distribution, score_format);

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
            className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4"
          >
            <Tile icon={<Clock aria-hidden />} label="Watch time">
              <CountUp
                value={stats.watch_time_minutes}
                format={humanizeMinutes}
              />
            </Tile>
            <Tile icon={<Tv aria-hidden />} label="Episodes">
              <CountUp value={stats.episode_plays} />
            </Tile>
            <Tile icon={<Film aria-hidden />} label="Films">
              <CountUp value={stats.movie_plays} />
            </Tile>
            {/* Completed watch-throughs: a finished rewatch counts again. */}
            <Tile icon={<Trophy aria-hidden />} label="Completions">
              <CountUp value={completed} />
            </Tile>
          </section>
          {(stats.longest_streak || stats.longest_binge) && (
            <section
              aria-label={`${year} records`}
              className="grid gap-3 sm:grid-cols-2"
            >
              {stats.longest_streak && (
                <Tile icon={<Flame aria-hidden />} label="Longest streak">
                  <span className="flex flex-col">
                    <CountUp
                      value={stats.longest_streak.days}
                      format={daysWord}
                    />
                    <span className="font-sans text-sm font-normal tracking-normal text-fg-muted">
                      {stats.longest_streak.days === 1
                        ? formatDay(stats.longest_streak.start, DAY)
                        : `${formatDay(stats.longest_streak.start, DAY)} to ${formatDay(stats.longest_streak.end, DAY)}`}
                    </span>
                  </span>
                </Tile>
              )}
              {stats.longest_binge && <BingeTile binge={stats.longest_binge} />}
            </section>
          )}
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
          <Section
            id="stats-scores"
            title="Scores"
            description={`How you scored what you completed in ${year}.`}
          >
            {scores.length === 0 ? (
              <p className="text-sm text-fg-muted">
                Nothing completed in {year} has a score yet.
              </p>
            ) : (
              <HorizontalBars
                label={`Scores in ${year}`}
                unit="titles"
                data={scores}
              />
            )}
          </Section>
          <TopRated titles={stats.top_rated} year={year} />
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
    </div>
  );
}

/** The most plays of one title on one day. */
function BingeTile({ binge }: { binge: Binge }) {
  const { title_language } = usePreferences();
  const name = displayTitle(binge.title, title_language);
  return (
    <Tile icon={<Popcorn aria-hidden />} label="Biggest binge">
      <span className="flex flex-col">
        <CountUp
          value={binge.plays}
          format={(n) => (n === 1 ? "1 play" : `${n} plays`)}
        />
        <span className="font-sans text-sm font-normal tracking-normal text-fg-muted">
          <Link
            to="/titles/$ref"
            params={{ ref: binge.title.ref }}
            className="font-medium text-fg hover:underline focus-visible:outline-2 focus-visible:outline-ring"
          >
            {name}
          </Link>{" "}
          on {formatDay(binge.date, DAY)}
        </span>
      </span>
    </Tile>
  );
}

/** The year's best-scored completed titles, as the API ranks them. */
function TopRated({
  titles,
  year,
}: {
  titles: readonly RatedTitle[];
  year: number;
}) {
  const { title_language, score_format } = usePreferences();
  if (titles.length === 0) return null;
  return (
    <Section
      id="stats-top-rated"
      title="Highest rated"
      description={`Your best-scored titles completed in ${year}.`}
    >
      <ol className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-5">
        {titles.slice(0, TOP_RATED).map(({ title, score }, index) => {
          const name = displayTitle(title, title_language);
          return (
            <li key={title.ref} className="flex flex-col gap-1.5">
              <Link
                to="/titles/$ref"
                params={{ ref: title.ref }}
                className="flex flex-col gap-1.5 rounded-poster focus-visible:outline-2 focus-visible:outline-ring"
              >
                <Poster
                  src={title.poster_url}
                  title={name}
                  className="w-full"
                />
                <span className="line-clamp-2 text-sm font-medium text-fg">
                  <span className="text-fg-subtle">{index + 1}. </span>
                  {name}
                </span>
              </Link>
              <Rating score={score} format={score_format} className="text-xs" />
            </li>
          );
        })}
      </ol>
    </Section>
  );
}
