// Every main screen, photographed in the light and the dark theme over the
// library the journey built, for a human to review (CI uploads
// artifacts/screenshots). Each screen must also render without an error
// state: a page that only "works" in one theme still fails here.

import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { type Browser, expect, type Page, test } from "@playwright/test";
import { signIn } from "./support/auth.ts";
import { stack, titles, users } from "./support/stack.ts";

const THEMES = ["light", "dark"] as const;

/** Screen name, then path. `{list}` is the journey's list. */
const SCREENS: readonly [string, string][] = [
  ["up-next", "/"],
  ["library", "/library"],
  ["title-show", `/titles/${encodeURIComponent(titles.show.ref)}`],
  ["title-movie", `/titles/${encodeURIComponent(titles.movie.ref)}`],
  ["discover", "/discover"],
  ["calendar", "/calendar"],
  ["lists", "/lists"],
  ["list", "/lists/{list}"],
  ["history", "/history"],
  ["stats", "/stats"],
  ["settings", "/settings"],
  ["settings-data", "/settings/data"],
  ["settings-import", "/settings/import"],
  ["settings-trash", "/settings/trash"],
  ["blocklist", "/blocklist"],
  ["admin", "/admin"],
];

async function settle(page: Page): Promise<void> {
  await page.waitForLoadState("networkidle");
  // Rails and long lists load as they scroll into view; bring every part of
  // the page on screen once so the full-page shot holds content, not
  // skeletons.
  const height = await page.evaluate(() => document.body.scrollHeight);
  for (let y = 0; y < height; y += 600) {
    await page.mouse.wheel(0, 600);
    await page.waitForTimeout(50);
  }
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => window.scrollTo(0, 0));
  // Skeletons announce themselves as loading statuses.
  await expect(
    page.locator('[role="status"][aria-label^="Loading"]'),
  ).toHaveCount(0);
  // Any error state, whatever its title (offline, provider unavailable, a
  // problem's own title, ...).
  await expect(page.getByTestId("error-state")).toHaveCount(0);
}

async function signedInPage(browser: Browser, theme: (typeof THEMES)[number]) {
  const context = await browser.newContext({
    baseURL: stack.web,
    colorScheme: theme,
    reducedMotion: "reduce",
    viewport: { width: 1440, height: 900 },
  });
  const page = await context.newPage();
  await signIn(page, users.first);
  return { context, page };
}

/** Collects every Content-Security-Policy violation the page reports. */
function cspViolations(page: Page): string[] {
  const violations: string[] = [];
  page.on("console", (message) => {
    if (/Content Security Policy/i.test(message.text())) {
      violations.push(message.text());
    }
  });
  return violations;
}

/** The three headers web/nginx.conf sends from every app location. */
function expectAppHeaders(headers: Record<string, string>, path: string) {
  expect(headers["content-security-policy"], path).toContain(
    "frame-ancestors 'none'",
  );
  expect(headers["x-content-type-options"], path).toBe("nosniff");
  expect(headers["referrer-policy"], path).toBe(
    "strict-origin-when-cross-origin",
  );
}

// nginx drops inherited add_headers in a location that sets its own, so each
// location is checked: /login (the `= /index.html` fallback), a fingerprinted
// asset, the service worker, the manifest and a plain file served by `/`.
test("every app location sends the security headers", async ({ request }) => {
  const login = await request.get(`${stack.web}/login`);
  expect(login.ok()).toBe(true);
  const loginHeaders = login.headers();
  expectAppHeaders(loginHeaders, "/login");
  const asset = /\/assets\/[^"']+\.js/.exec(await login.text())?.[0];
  expect(asset, "index.html links a bundle under /assets/").toBeTruthy();

  for (const path of [
    asset ?? "",
    "/sw.js",
    "/manifest.webmanifest",
    "/icons/icon.svg",
  ]) {
    const response = await request.get(`${stack.web}${path}`);
    expect(response.ok(), path).toBe(true);
    expectAppHeaders(response.headers(), path);
  }

  // The API sends its own headers; the app's policy must not reach it.
  const api = await request.get(`${stack.web}/api/v1/me`);
  expect(api.headers()["content-security-policy"]).not.toBe(
    loginHeaders["content-security-policy"],
  );
});

for (const theme of THEMES) {
  test(`main screens in the ${theme} theme`, async ({ browser }) => {
    test.setTimeout(180_000);
    const dir = join(stack.screenshots, theme);
    await mkdir(dir, { recursive: true });

    const signedOut = await browser.newContext({
      baseURL: stack.web,
      colorScheme: theme,
      reducedMotion: "reduce",
      viewport: { width: 1440, height: 900 },
    });
    const login = await signedOut.newPage();
    const loginViolations = cspViolations(login);
    await login.goto("/login");
    await expect(login.getByTestId("login-page")).toBeVisible();
    await login.screenshot({ path: join(dir, "login.png"), fullPage: true });
    await signedOut.close();
    expect(loginViolations, "the login page obeys its CSP").toEqual([]);

    const { context, page } = await signedInPage(browser, theme);
    const violations = cspViolations(page);
    const lists = await page.request.get("/api/v1/lists");
    expect(lists.ok()).toBe(true);
    const listId = ((await lists.json()) as { items: { id: string }[] })
      .items[0]?.id;
    expect(listId, "the journey left a list").toBeTruthy();

    for (const [name, path] of SCREENS) {
      await test.step(name, async () => {
        await page.goto(path.replace("{list}", listId ?? ""));
        await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
        await settle(page);
        await page.screenshot({
          path: join(dir, `${name}.png`),
          fullPage: true,
        });
      });
    }

    await test.step("search palette", async () => {
      await page.goto("/library");
      await page.getByRole("button", { name: /Search titles/ }).click();
      await page.getByRole("combobox", { name: "Search titles" }).fill("the");
      await expect(page.getByRole("option").first()).toBeVisible();
      await page.screenshot({ path: join(dir, "search.png") });
    });
    await context.close();
    expect(violations, "every screen obeys the app's CSP").toEqual([]);
  });
}
