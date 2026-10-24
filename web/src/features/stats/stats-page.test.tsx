// Stats in the real app: every section of a year in review for a fixture
// year, the empty year, the year selector, count-ups under reduced motion,
// and the charts in both themes.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { Stats, TitleSummary } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/features/library/axe";
import { setMediaQueries } from "@/test/setup";
import {
  json,
  makeSession,
  mockApi,
  renderApp,
  requestsOf,
} from "@/test/utils";
import { CHART_TOKENS, HorizontalBars } from "./charts";
import { CountUp } from "./count-up";
import { scoreBuckets, yearsOf } from "./stats-page";

const YEAR = 2025;

const LIBRARY = {
  by_status: [
    { status: "completed", count: 12 },
    { status: "watching", count: 3 },
  ],
  by_category: [
    { category: "anime", count: 9 },
    { category: "movie", count: 6 },
  ],
} as const;

const FRIEREN: TitleSummary = {
  ref: "anilist:show:154587",
  title: "Frieren",
  kind: "show",
  category: "anime",
  genres: [],
  library: null,
};

const FULL: Stats = {
  year: YEAR,
  ...LIBRARY,
  by_status: [...LIBRARY.by_status],
  by_category: [...LIBRARY.by_category],
  watch_time_minutes: 1500,
  completions_by_year: [{ year: YEAR, completed: 7 }],
  monthly_activity: [
    { month: "2025-01", plays: 4 },
    { month: "2025-03", plays: 10 },
  ],
  top_genres: [
    { genre: "Fantasy", count: 5 },
    { genre: "Drama", count: 2 },
  ],
  episode_plays: 11,
  movie_plays: 3,
  score_distribution: [
    { score: 80, count: 2 },
    { score: 95, count: 1 },
  ],
  top_rated: [{ title: FRIEREN, score: 95 }],
  longest_streak: { days: 5, start: "2025-03-01", end: "2025-03-05" },
  longest_binge: { title: FRIEREN, date: "2025-03-02", plays: 6 },
};

const EMPTY: Stats = {
  ...FULL,
  year: 2024,
  watch_time_minutes: 0,
  completions_by_year: [],
  monthly_activity: [],
  top_genres: [],
  episode_plays: 0,
  movie_plays: 0,
  score_distribution: [],
  top_rated: [],
  longest_streak: null,
  longest_binge: null,
};

const ALL_TIME: Stats = {
  ...FULL,
  year: null,
  completions_by_year: [
    { year: 2024, completed: 0 },
    { year: YEAR, completed: 7 },
  ],
};

function statsServer() {
  return mockApi((request) => {
    const url = new URL(request.url);
    const path = url.pathname.replace(/^\/api\/v1/, "");
    if (path === "/sessions/current") return json(makeSession());
    if (path === "/stats") {
      const year = url.searchParams.get("year");
      if (!year) return json(ALL_TIME);
      return json(
        Number(year) === YEAR ? FULL : { ...EMPTY, year: Number(year) },
      );
    }
    return undefined;
  });
}

describe("stats", () => {
  test("a fixture year renders every section", async () => {
    const api = statsServer();
    const { container } = renderApp(`/stats?year=${YEAR}`);
    const totals = await screen.findByRole("region", {
      name: `${YEAR} totals`,
    });
    expect(totals).toHaveTextContent("1d 1h");
    // Episodes and films apart, then completions; each count-up announces
    // its final figure.
    const final = (text: string) =>
      within(totals).getByText(text, { selector: ".sr-only" });
    expect(final("11")).toBeInTheDocument();
    expect(final("3")).toBeInTheDocument();
    expect(final("7")).toBeInTheDocument();
    expect(totals).toHaveTextContent(/Episodes.*Films.*Completions/);

    const records = screen.getByRole("region", { name: `${YEAR} records` });
    expect(
      within(records).getByText("5 days", { selector: ".sr-only" }),
    ).toBeInTheDocument();
    expect(records).toHaveTextContent("Mar 1 to Mar 5");
    expect(
      within(records).getByText("6 plays", { selector: ".sr-only" }),
    ).toBeInTheDocument();
    expect(
      within(records).getByRole("link", { name: "Frieren" }),
    ).toHaveAttribute("href", `/titles/${encodeURIComponent(FRIEREN.ref)}`);
    expect(records).toHaveTextContent("Frieren on Mar 2");

    // Scores in the user's format (10-point with a decimal), highest first,
    // with the empty 9 between.
    expect(
      screen.getByRole("table", { name: `Scores in ${YEAR}` }),
    ).toHaveTextContent("10.01 titles9.00 titles8.02 titles");

    const months = screen.getByRole("list", {
      name: `Plays per month in ${YEAR}`,
    });
    expect(within(months).getAllByRole("listitem")).toHaveLength(12);
    expect(
      within(months).getByRole("listitem", { name: /: 10 plays$/ }),
    ).toBeInTheDocument();

    expect(
      screen.getByRole("table", { name: `Top genres in ${YEAR}` }),
    ).toHaveTextContent("Fantasy5 titles");
    expect(
      screen.getByRole("table", { name: "Library titles per category" }),
    ).toHaveTextContent("Anime9 titles");
    expect(
      screen.getByRole("table", { name: "Library titles per status" }),
    ).toHaveTextContent("Completed12 titles");
    const top = screen.getByRole("region", { name: "Highest rated" });
    expect(top).toHaveTextContent(`completed in ${YEAR}`);
    expect(
      within(top).getByRole("link", { name: /Frieren/ }),
    ).toBeInTheDocument();
    // The year's top-rated come with the stats, not from the library.
    expect(requestsOf(api).some((r) => r.includes("/library"))).toBe(false);
    await expectNoSeriousA11yViolations(container);
  });

  test("a year without scores or dated plays says so", async () => {
    mockApi((request) => {
      const url = new URL(request.url);
      const path = url.pathname.replace(/^\/api\/v1/, "");
      if (path === "/sessions/current") return json(makeSession());
      if (path === "/stats") {
        return json({
          ...EMPTY,
          year: YEAR,
          watch_time_minutes: 90,
          movie_plays: 1,
        });
      }
      return undefined;
    });
    renderApp(`/stats?year=${YEAR}`);
    await screen.findByRole("region", { name: `${YEAR} totals` });
    expect(
      screen.getByText(`Nothing completed in ${YEAR} has a score yet.`),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: `${YEAR} records` }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "Highest rated" }),
    ).not.toBeInTheDocument();
  });

  test("scores are binned to the user's format", () => {
    const scores = [
      { score: 0, count: 1 },
      { score: 55, count: 2 },
      { score: 61, count: 1 },
      { score: 100, count: 3 },
    ];
    // Finer than ten points: binned to ten.
    expect(scoreBuckets(scores, "point_100").map((b) => b.label)).toEqual([
      "100",
      "90",
      "80",
      "70",
      "60",
      "50",
      "40",
      "30",
      "20",
      "10",
      "0",
    ]);
    expect(scoreBuckets(scores, "point_100")[4]).toEqual({
      label: "60",
      value: 3,
    });
    expect(scoreBuckets(scores, "point_5")).toEqual([
      { label: "5 / 5", value: 3 },
      { label: "4 / 5", value: 0 },
      { label: "3 / 5", value: 3 },
      { label: "2 / 5", value: 0 },
      { label: "1 / 5", value: 0 },
      { label: "0 / 5", value: 1 },
    ]);
    expect(scoreBuckets(scores, "point_3")).toEqual([
      { label: "Liked", value: 3 },
      { label: "Okay", value: 3 },
      { label: "Disliked", value: 1 },
    ]);
    expect(scoreBuckets([], "point_10")).toEqual([]);
  });

  test("an empty year shows an empty state", async () => {
    statsServer();
    renderApp("/stats?year=2024");
    expect(
      await screen.findByRole("heading", { name: "Nothing logged in 2024" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "2024 totals" }),
    ).not.toBeInTheDocument();
    // The library as it stands is still shown.
    expect(
      screen.getByRole("heading", { name: "Your library by category" }),
    ).toBeInTheDocument();
  });

  test("the year selector asks for that year", async () => {
    const api = statsServer();
    const user = userEvent.setup();
    const { router } = renderApp(`/stats?year=${YEAR}`);
    await screen.findByRole("region", { name: `${YEAR} totals` });
    await user.click(screen.getByRole("combobox", { name: "Year" }));
    await user.click(await screen.findByRole("option", { name: "2024" }));
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ year: 2024 }),
    );
    await waitFor(() =>
      expect(requestsOf(api)).toContain("GET /api/v1/stats?year=2024"),
    );
  });

  test("the years offered are those with activity and this one", () => {
    expect(yearsOf(ALL_TIME, 2026)).toEqual([2026, 2025, 2024]);
  });
});

describe("count-up", () => {
  test("shows the final figure at once under reduced motion", () => {
    setMediaQueries(["(prefers-reduced-motion: reduce)"]);
    const { container } = render(<CountUp value={1234} />);
    expect(container.querySelector("[aria-hidden]")).toHaveTextContent("1,234");
  });

  test("announces the final figure while it animates", () => {
    render(<CountUp value={42} />);
    expect(
      screen.getByText("42", { selector: ".sr-only" }),
    ).toBeInTheDocument();
  });
});

/** The OKLCH lightness (0–100) of each token in a theme block of tokens.css. */
function lightness(block: string): Map<string, number> {
  const values = new Map<string, number>();
  for (const [, name, l] of block.matchAll(/--([\w-]+):\s*oklch\(([\d.]+)%/g)) {
    if (name && l) values.set(name, Number(l));
  }
  return values;
}

describe("charts in both themes", () => {
  const css = readFileSync(
    join(import.meta.dirname, "../../styles/tokens.css"),
    "utf8",
  );
  // The blocks themselves, not the header comment that names them.
  const lightAt = css.indexOf("\n:root {");
  const darkAt = css.indexOf('\n[data-theme="dark"] {');
  const light = lightness(css.slice(lightAt, darkAt));
  const dark = lightness(css.slice(darkAt, css.indexOf("\n@theme", darkAt)));

  test.each([
    ["light", light],
    ["dark", dark],
  ])(
    "marks and labels stand off the surface in the %s theme",
    (_name, theme) => {
      const surface = theme.get("surface") ?? Number.NaN;
      // Bars: a clear step from the card; text: a strong one.
      expect(
        Math.abs((theme.get("accent") ?? 0) - surface),
      ).toBeGreaterThanOrEqual(30);
      expect(Math.abs((theme.get("fg") ?? 0) - surface)).toBeGreaterThanOrEqual(
        60,
      );
      expect(
        Math.abs((theme.get("fg-muted") ?? 0) - surface),
      ).toBeGreaterThanOrEqual(40);
    },
  );

  test("every chart colour is a theme token", () => {
    for (const value of Object.values(CHART_TOKENS)) {
      expect(value).toMatch(/^var\(--color-[\w-]+\)$/);
    }
  });

  test.each(["light", "dark"])("the bar chart in the %s theme", (theme) => {
    document.documentElement.dataset.theme = theme;
    const { container } = render(
      <HorizontalBars
        label="Genres"
        unit="titles"
        data={[
          { label: "Fantasy", value: 5 },
          { label: "Drama", value: 2 },
        ]}
      />,
    );
    const bars = [...container.querySelectorAll(".recharts-rectangle")];
    expect(bars).toHaveLength(2);
    // Painted with tokens only, so the theme decides the colour.
    expect(bars.map((bar) => bar.getAttribute("fill"))).toEqual([
      CHART_TOKENS.mark,
      CHART_TOKENS.mark,
    ]);
    expect(
      [...container.querySelectorAll(".recharts-label")].map(
        (l) => l.textContent,
      ),
    ).toEqual(["5", "2"]);
    // Recharts numbers its ids per render; they say nothing about the look.
    const svg = container
      .querySelector("[data-chart]")
      ?.innerHTML.replace(/\b(id|clip-path)="[^"]*"/g, '$1="…"');
    expect(svg).toMatchSnapshot();
  });
});
