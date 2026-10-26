// Outage drills (docs/QA.md, "Outage drills"): the metadata upstreams fail
// and the database goes away mid-session, over the library the journey
// built. The library must keep serving from its snapshots, every failure must
// be classified (never a 500), and nothing the person tracked may be lost.

import { expect, test } from "@playwright/test";
import { signIn } from "./support/auth.ts";
import {
  database,
  readiness,
  serverErrors,
  setUpstream,
  total,
  watchApi,
} from "./support/drills.ts";
import { titles, users } from "./support/stack.ts";

const STALE = /A metadata provider isn't answering/;

test.afterEach(async () => {
  await setUpstream("tmdb", "ok");
  await setUpstream("anilist", "ok");
});

test("TMDB down: the library renders from snapshots and nothing 500s", async ({
  page,
}) => {
  await signIn(page, users.first);
  const seen = watchApi(page);
  const errorsBefore = await total("stacks_metadata_errors_total", {
    provider: "tmdb",
  });
  const http500Before = await total("stacks_http_requests_total", {
    status: "500",
  });

  await setUpstream("tmdb", "error");

  await test.step("the library lists every tracked title", async () => {
    await page.goto("/library");
    await expect(page.getByText(titles.show.name).first()).toBeVisible();
    await expect(page.getByText(titles.movie.name).first()).toBeVisible();
  });

  await test.step("title detail comes from the snapshot, marked stale", async () => {
    await page.goto(`/titles/${encodeURIComponent(titles.show.ref)}`);
    await expect(
      page.getByRole("heading", { level: 1, name: titles.show.name }),
    ).toBeVisible();
    await expect(page.getByText(STALE)).toBeVisible();
    await expect(page.getByText(/\d+ of 4 episodes/)).toBeVisible();
  });

  await test.step("stats still render", async () => {
    await page.goto("/stats");
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Something went wrong" }),
    ).toHaveCount(0);
  });

  await test.step("Discover rails fail one by one, the page stays up", async () => {
    await page.goto("/discover");
    await expect(
      page.getByRole("heading", { level: 1, name: "Discover" }),
    ).toBeVisible();
    await expect(
      page.getByText("A metadata provider is unavailable").first(),
    ).toBeVisible();
  });

  await test.step("errors are classified, counted, and never a 500", async () => {
    expect(serverErrors(seen)).toEqual([]);
    expect(
      await total("stacks_metadata_errors_total", { provider: "tmdb" }),
    ).toBeGreaterThan(errorsBefore);
    expect(await total("stacks_http_requests_total", { status: "500" })).toBe(
      http500Before,
    );
  });
});

test("AniList rate-limited: the UI says try again shortly", async ({
  page,
}) => {
  await signIn(page, users.first);
  const seen = watchApi(page);
  const before = await total("stacks_metadata_errors_total", {
    provider: "anilist",
    class: "rate_limited",
  });

  await setUpstream("anilist", "rate_limited", 3);

  // Never tracked, so there is no snapshot to fall back on.
  await page.goto(`/titles/${encodeURIComponent(titles.anime.ref)}`);
  // The web client retries a retryable problem once, after its Retry-After.
  await expect(page.getByText("A metadata provider is busy")).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByText(/Try again in \d+ s\./)).toBeVisible();

  expect(serverErrors(seen)).toEqual([]);
  await expect
    .poll(() =>
      total("stacks_metadata_errors_total", {
        provider: "anilist",
        class: "rate_limited",
      }),
    )
    .toBeGreaterThan(before);

  await setUpstream("anilist", "ok");
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(
    page.getByRole("heading", { level: 1, name: titles.anime.name }),
  ).toBeVisible();
});

test("database restart mid-session: readiness flips back and no data is lost", async ({
  page,
}) => {
  test.setTimeout(180_000);
  await signIn(page, users.first);
  const before = await page.request.get("/api/v1/exports/backup");
  expect(before.ok()).toBe(true);
  const { entries: entriesBefore } = (await before.json()) as {
    entries: unknown[];
  };

  try {
    database("stop");
    await readiness(503, 30_000);
  } finally {
    database("start");
  }
  await readiness(200, 90_000);

  // The same session cookie still works: sessions live in the database.
  await page.goto("/library");
  await expect(page.getByText(titles.show.name).first()).toBeVisible();
  const after = await page.request.get("/api/v1/exports/backup");
  expect(after.ok()).toBe(true);
  const { entries: entriesAfter } = (await after.json()) as {
    entries: unknown[];
  };
  expect(entriesAfter).toEqual(entriesBefore);

  // And it still writes.
  await page.goto(`/titles/${encodeURIComponent(titles.movie.ref)}`);
  await page
    .getByRole("button", { name: "Watched again", exact: true })
    .click();
  await expect(page.getByText("Watched", { exact: true })).toBeVisible();
});
