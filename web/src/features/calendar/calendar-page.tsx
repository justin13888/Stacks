import { infiniteQueryOptions, useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { CalendarDays, Check, ChevronLeft, ChevronRight } from "lucide-react";
import { useEffect, useState } from "react";
import { getListCalendarQueryKey, listCalendar } from "@/api/library/library";
import type {
  CalendarEpisode,
  ListCalendarParams,
  PageCalendarEpisode,
} from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { IconButton } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { episodeLabel } from "@/features/titles/episodes";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { cn } from "@/lib/cn";
import { formatDay, today } from "@/lib/format";
import {
  addDays,
  addMonths,
  formatMonth,
  groupByDay,
  monthGrid,
  startOfMonth,
} from "./dates";

export type CalendarMode = "month" | "agenda";

/** The agenda shows the past week and the next four. */
export const AGENDA_PAST_DAYS = 7;
export const AGENDA_FUTURE_DAYS = 28;

function calendarOptions(params: ListCalendarParams) {
  return infiniteQueryOptions({
    queryKey: [...getListCalendarQueryKey(params), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listCalendar({ ...params, cursor: pageParam, limit: 200 }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageCalendarEpisode) =>
      last.next_cursor ?? undefined,
  });
}

/** Every episode airing from `from` to `to`, all pages of it. */
function useCalendar(from: string, to: string) {
  const query = useInfiniteQuery(calendarOptions({ from, to }));
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = query;
  // A range is shown whole: keep reading until the last page is in.
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) void fetchNextPage();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);
  return query;
}

/**
 * Air dates of the shows in the library: a month grid, or an agenda of the
 * recent and coming weeks. Air dates are calendar days and are shown as the
 * day they name, wherever the user is.
 */
export function CalendarPage() {
  const now = today();
  const [mode, setMode] = useState<CalendarMode>("month");
  const [month, setMonth] = useState(startOfMonth(now));
  const days = monthGrid(month);
  const from =
    mode === "month" ? (days[0] ?? month) : addDays(now, -AGENDA_PAST_DAYS);
  const to =
    mode === "month"
      ? (days.at(-1) ?? month)
      : addDays(now, AGENDA_FUTURE_DAYS);
  const calendar = useCalendar(from, to);
  const episodes = calendar.data?.pages.flatMap((page) => page.items) ?? [];

  return (
    <>
      <PageHeader
        title="Calendar"
        description="When the next episodes of everything you follow air."
        actions={
          <fieldset className="inline-flex items-center gap-1 rounded-lg bg-surface-sunken p-1">
            <legend className="sr-only">Calendar view</legend>
            {(["month", "agenda"] as const).map((m) => (
              <button
                key={m}
                type="button"
                aria-pressed={mode === m}
                onClick={() => setMode(m)}
                className={cn(
                  "inline-flex h-8 items-center rounded-md px-3 text-sm font-medium text-fg-muted",
                  "hover:text-fg focus-visible:outline-2 focus-visible:outline-ring",
                  mode === m && "bg-surface text-fg shadow-raised",
                )}
              >
                {m === "month" ? "Month" : "Agenda"}
              </button>
            ))}
          </fieldset>
        }
      />
      {mode === "month" && (
        <div className="mb-4 flex items-center gap-2">
          <IconButton
            label="Previous month"
            icon={<ChevronLeft aria-hidden className="size-5" />}
            onClick={() => setMonth(addMonths(month, -1))}
          />
          <h2
            aria-live="polite"
            className="min-w-40 text-center font-display text-lg font-semibold text-fg"
          >
            {formatMonth(month)}
          </h2>
          <IconButton
            label="Next month"
            icon={<ChevronRight aria-hidden className="size-5" />}
            onClick={() => setMonth(addMonths(month, 1))}
          />
          {month !== startOfMonth(now) && (
            <button
              type="button"
              onClick={() => setMonth(startOfMonth(now))}
              className="ml-2 rounded-sm text-sm font-medium text-accent hover:underline focus-visible:outline-2 focus-visible:outline-ring"
            >
              Today
            </button>
          )}
        </div>
      )}
      {calendar.isPending ? (
        <Skeleton className="h-96" />
      ) : calendar.isError ? (
        <ErrorState
          error={calendar.error}
          onRetry={() => void calendar.refetch()}
        />
      ) : mode === "month" ? (
        <MonthGrid month={month} days={days} today={now} episodes={episodes} />
      ) : (
        <Agenda today={now} episodes={episodes} />
      )}
    </>
  );
}

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function MonthGrid({
  month,
  days,
  today,
  episodes,
}: {
  month: string;
  days: readonly string[];
  today: string;
  episodes: readonly CalendarEpisode[];
}) {
  const byDay = new Map(groupByDay(episodes, (e) => e.air_date));
  const inMonth = month.slice(0, 7);
  const weeks = Array.from({ length: days.length / 7 }, (_, week) =>
    days.slice(week * 7, week * 7 + 7),
  );
  return (
    <div className="overflow-hidden rounded-xl border border-line">
      <table className="w-full table-fixed border-collapse">
        <caption className="sr-only">{formatMonth(month)}</caption>
        <thead className="bg-surface-sunken text-xs font-medium text-fg-muted">
          <tr>
            {WEEKDAYS.map((d) => (
              <th key={d} scope="col" className="py-2 font-medium">
                {d}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {weeks.map((week) => (
            <tr key={week[0]}>
              {week.map((day) => {
                const items = byDay.get(day) ?? [];
                return (
                  <td
                    key={day}
                    data-day={day}
                    aria-current={day === today ? "date" : undefined}
                    className={cn(
                      "h-16 min-w-0 border-t border-l border-line p-1 align-top first:border-l-0 sm:h-24 sm:p-1.5",
                      !day.startsWith(inMonth) &&
                        "bg-surface-sunken/50 text-fg-subtle",
                    )}
                  >
                    <div className="flex min-w-0 flex-col gap-1">
                      <span
                        aria-hidden
                        className={cn(
                          "inline-flex size-6 items-center justify-center self-end rounded-full text-xs tabular-nums",
                          day === today &&
                            "bg-accent font-semibold text-fg-on-accent",
                        )}
                      >
                        {Number(day.slice(8))}
                      </span>
                      <span className="sr-only">
                        {formatDay(day, {
                          weekday: "long",
                          month: "long",
                          day: "numeric",
                        })}
                      </span>
                      {items.length > 0 && (
                        <ul className="flex min-w-0 flex-col gap-0.5">
                          {items.map((item) => (
                            <li
                              key={`${item.title.ref}:${item.season}:${item.episode}`}
                              className="min-w-0"
                            >
                              <EpisodeChip item={item} />
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function EpisodeChip({ item }: { item: CalendarEpisode }) {
  const { title_language } = usePreferences();
  const name = displayTitle(item.title, title_language);
  return (
    <Link
      to="/titles/$ref"
      params={{ ref: item.title.ref }}
      title={`${name} ${episodeLabel(item)}`}
      className={cn(
        "flex items-center gap-1 truncate rounded-sm px-1 py-0.5 text-2xs font-medium focus-visible:outline-2 focus-visible:outline-ring sm:text-xs",
        item.watched ? "bg-success/12 text-fg-muted" : "bg-accent-soft text-fg",
      )}
    >
      {item.watched && (
        <Check aria-label="Watched" className="size-3 shrink-0" />
      )}
      <span className="truncate">{name}</span>
      <span className="hidden shrink-0 tabular-nums text-fg-muted sm:inline">
        {episodeLabel(item)}
      </span>
    </Link>
  );
}

function Agenda({
  today,
  episodes,
}: {
  today: string;
  episodes: readonly CalendarEpisode[];
}) {
  const { title_language } = usePreferences();
  const groups = groupByDay(episodes, (e) => e.air_date);
  if (groups.length === 0) {
    return (
      <EmptyState
        icon={<CalendarDays />}
        title="Nothing airing these weeks"
        description="Episodes of the shows in your library appear here as their air dates are announced."
      />
    );
  }
  return (
    <ol className="flex flex-col gap-6">
      {groups.map(([day, items]) => (
        <li key={day}>
          <section
            aria-labelledby={`day-${day}`}
            className="flex flex-col gap-2"
          >
            <h2
              id={`day-${day}`}
              className={cn(
                "text-sm font-semibold",
                day === today
                  ? "text-accent"
                  : day < today
                    ? "text-fg-muted"
                    : "text-fg",
              )}
            >
              {day === today ? "Today · " : ""}
              {formatDay(day, {
                weekday: "long",
                month: "long",
                day: "numeric",
              })}
            </h2>
            <ul className="flex flex-col gap-2">
              {items.map((item) => {
                const name = displayTitle(item.title, title_language);
                return (
                  <li key={`${item.title.ref}:${item.season}:${item.episode}`}>
                    <Link
                      to="/titles/$ref"
                      params={{ ref: item.title.ref }}
                      className="flex items-center gap-3 rounded-lg border border-line bg-surface p-3 hover:bg-surface-raised focus-visible:outline-2 focus-visible:outline-ring"
                    >
                      <span className="flex min-w-0 flex-1 flex-col">
                        <span className="truncate text-sm font-medium text-fg">
                          {name}
                        </span>
                        <span className="truncate text-xs text-fg-muted">
                          {episodeLabel(item)}
                          {item.name ? ` · ${item.name}` : ""}
                          {item.runtime_minutes
                            ? ` · ${item.runtime_minutes} min`
                            : ""}
                        </span>
                      </span>
                      {item.watched && (
                        <span className="inline-flex items-center gap-1 text-xs text-success">
                          <Check aria-hidden className="size-4" />
                          Watched
                        </span>
                      )}
                    </Link>
                  </li>
                );
              })}
            </ul>
          </section>
        </li>
      ))}
    </ol>
  );
}
