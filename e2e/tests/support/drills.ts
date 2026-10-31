import { execSync } from "node:child_process";
import { expect, type Page, type Response } from "@playwright/test";
import { stack } from "./stack.ts";

type Mode = "ok" | "error" | "rate_limited";

/** Flips one upstream of the fixture server (fixtures/server.ts). */
export async function setUpstream(
  upstream: "tmdb" | "anilist",
  mode: Mode,
  retryAfter = 30,
): Promise<void> {
  const res = await fetch(`${stack.fixtures}/__control/${upstream}`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ mode, retryAfter }),
  });
  expect(res.ok, `fixture control for ${upstream}`).toBe(true);
}

/** One Prometheus sample: its labels and value. */
interface Sample {
  labels: Record<string, string>;
  value: number;
}

/** Scrapes `/metrics` and returns every sample of `name`. */
export async function scrape(name: string): Promise<Sample[]> {
  const res = await fetch(stack.metrics);
  expect(res.ok, "the operator listener answers").toBe(true);
  const text = await res.text();
  const samples: Sample[] = [];
  for (const line of text.split("\n")) {
    if (!line.startsWith(`${name}{`) && !line.startsWith(`${name} `)) continue;
    const match = /^[^{\s]+(?:\{(.*)\})?\s+(\S+)/.exec(line);
    if (!match) continue;
    const labels: Record<string, string> = {};
    for (const pair of (match[1] ?? "").matchAll(/(\w+)="([^"]*)"/g)) {
      labels[pair[1] as string] = pair[2] as string;
    }
    samples.push({ labels, value: Number(match[2]) });
  }
  return samples;
}

/** The sum of the samples of `name` whose labels include `where`. */
export async function total(
  name: string,
  where: Record<string, string> = {},
): Promise<number> {
  return (await scrape(name))
    .filter((s) => Object.entries(where).every(([k, v]) => s.labels[k] === v))
    .reduce((sum, s) => sum + s.value, 0);
}

/**
 * Records every API response a page receives, so a drill can assert that
 * none of them was a 500: an outage must surface as a classified 502/503 or
 * a stale 200, never as an unhandled error.
 */
export function watchApi(page: Page): Response[] {
  const seen: Response[] = [];
  page.on("response", (res) => {
    if (new URL(res.url()).pathname.startsWith("/api/")) seen.push(res);
  });
  return seen;
}

export function serverErrors(seen: Response[]): string[] {
  return seen
    .filter((r) => r.status() === 500)
    .map((r) => `${r.request().method()} ${r.url()} -> 500`);
}

/** Polls `/readyz` until it answers `status`, within `withinMs`. */
export async function readiness(status: number, withinMs: number) {
  await expect
    .poll(
      async () => {
        try {
          const res = await fetch(`${stack.api}/readyz`, {
            signal: AbortSignal.timeout(15_000),
          });
          return res.status;
        } catch {
          return 0;
        }
      },
      { timeout: withinMs, intervals: [250, 500, 1_000] },
    )
    .toBe(status);
}

/** Runs the runner's command that stops or starts PostgreSQL. */
export function database(action: "stop" | "start"): void {
  const command = action === "stop" ? stack.stopDatabase : stack.startDatabase;
  if (!command) {
    throw new Error(
      `E2E_${action.toUpperCase()}_DATABASE is not set; \`mise run e2e\` sets it to the compose ${action} of postgres`,
    );
  }
  execSync(command, { stdio: "inherit", timeout: 120_000 });
}
