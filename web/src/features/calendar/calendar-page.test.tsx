// The calendar in the real app, in a zone west of Greenwich: air dates are
// calendar days and must land on the day they name, not the day before.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  test,
  vi,
} from "vitest";
import type { CalendarEpisode } from "@/api/schemas";
import { parseDay, toDay } from "@/lib/format";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import { fakeApi, makeTitle } from "@/test/fake-api";
import { json, renderApp, requestsOf } from "@/test/utils";
import { addDays, groupByDay, monthGrid } from "./dates";

const SHOW = makeTitle({ ref: "tmdb:show:1396", title: "Breaking Bad" });
const ANIME = makeTitle({
  ref: "anilist:show:154587",
  title: "Frieren",
  category: "anime",
});

function episode(
  title: typeof SHOW,
  air_date: string,
  number: number,
  watched = false,
): CalendarEpisode {
  return { title, season: 1, episode: number, air_date, watched };
}

const EPISODES = [
  episode(SHOW, "2026-10-01", 1, true),
  episode(ANIME, "2026-10-24", 7),
  episode(SHOW, "2026-10-24", 2),
];

let zone: string | undefined;
beforeAll(() => {
  zone = process.env.TZ;
  process.env.TZ = "America/Los_Angeles";
});
afterAll(() => {
  process.env.TZ = zone;
});
beforeEach(() => {
  // Only the clock is fake; timers stay real for the UI.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 9, 20, 9, 0));
});
afterEach(() => {
  vi.useRealTimers();
});

function calendarServer(
  pages?: Record<string, { items: CalendarEpisode[]; next?: string }>,
) {
  return fakeApi({
    titles: [SHOW, ANIME],
    extra: (request, path) => {
      if (path !== "/calendar") return undefined;
      const params = new URL(request.url).searchParams;
      if (pages) {
        const page = pages[params.get("cursor") ?? "first"];
        return json({
          items: page?.items ?? [],
          next_cursor: page?.next ?? null,
        });
      }
      const from = params.get("from") ?? "";
      const to = params.get("to") ?? "";
      return json({
        items: EPISODES.filter((e) => e.air_date >= from && e.air_date <= to),
        next_cursor: null,
      });
    },
  });
}

function calendarRequests(fake: ReturnType<typeof calendarServer>) {
  return requestsOf(fake.api).filter((r) => r.includes("/calendar"));
}

describe("calendar dates", () => {
  test("the test zone is west of Greenwich", () => {
    expect(new Date(2026, 9, 24).getTimezoneOffset()).toBeGreaterThan(0);
  });

  test("a calendar day is read as that local day", () => {
    const day = parseDay("2026-10-24");
    expect(day?.getDate()).toBe(24);
    expect(day && toDay(day)).toBe("2026-10-24");
    expect(parseDay("24/10/2026")).toBeNull();
  });

  test("days move across months and the DST change", () => {
    expect(addDays("2026-10-31", 1)).toBe("2026-11-01");
    expect(addDays("2026-11-01", 1)).toBe("2026-11-02");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
  });

  test("a month grid is whole Monday-first weeks", () => {
    const days = monthGrid("2026-10-15");
    expect(days[0]).toBe("2026-09-28");
    expect(days.at(-1)).toBe("2026-11-01");
    expect(days).toHaveLength(35);
  });

  test("grouping keeps days in order", () => {
    expect(
      groupByDay(EPISODES, (e) => e.air_date).map(([day, items]) => [
        day,
        items.length,
      ]),
    ).toEqual([
      ["2026-10-01", 1],
      ["2026-10-24", 2],
    ]);
  });
});

describe("calendar page", () => {
  test("the month view asks for its whole grid and groups by local date", async () => {
    const fake = calendarServer();
    const { container } = renderApp("/calendar");
    const saturday = (await screen.findByText("Saturday, October 24")).closest(
      "td",
    ) as HTMLElement;
    await within(saturday).findByText("Frieren");
    expect(within(saturday).getByText("Breaking Bad")).toBeInTheDocument();
    const friday = screen
      .getByText("Friday, October 23")
      .closest("td") as HTMLElement;
    expect(within(friday).queryByRole("link")).toBeNull();
    expect(calendarRequests(fake)).toEqual([
      "GET /api/v1/calendar?from=2026-09-28&to=2026-11-01&limit=200",
    ]);
    expect(
      screen.getByText("Tuesday, October 20").closest("td"),
    ).toHaveAttribute("aria-current", "date");
    await expectNoSeriousA11yViolations(container);
  });

  test("the month can be changed", async () => {
    const fake = calendarServer();
    const user = userEvent.setup();
    renderApp("/calendar");
    await screen.findByRole("heading", { name: "October 2026" });
    await user.click(screen.getByRole("button", { name: "Next month" }));
    await screen.findByRole("heading", { name: "November 2026" });
    await waitFor(() =>
      expect(calendarRequests(fake).at(-1)).toBe(
        "GET /api/v1/calendar?from=2026-10-26&to=2026-12-06&limit=200",
      ),
    );
  });

  test("the agenda groups the recent and coming weeks by day", async () => {
    const fake = calendarServer();
    const user = userEvent.setup();
    renderApp("/calendar");
    await user.click(await screen.findByRole("button", { name: "Agenda" }));
    const day = await screen.findByRole("region", {
      name: "Saturday, October 24",
    });
    expect(within(day).getAllByRole("link")).toHaveLength(2);
    expect(calendarRequests(fake).at(-1)).toBe(
      "GET /api/v1/calendar?from=2026-10-13&to=2026-11-17&limit=200",
    );
  });

  test("every page of a range is read", async () => {
    const fake = calendarServer({
      first: { items: [EPISODES[1] as CalendarEpisode], next: "p2" },
      p2: { items: [EPISODES[2] as CalendarEpisode] },
    });
    renderApp("/calendar");
    const saturday = (await screen.findByText("Saturday, October 24")).closest(
      "td",
    ) as HTMLElement;
    await within(saturday).findByText("Breaking Bad");
    expect(calendarRequests(fake)).toHaveLength(2);
  });
});
