// The rule every web change keeps: only the orval-generated client talks to
// the API. Its one mutator (src/lib/api-client.ts) is the only `fetch`, and no
// page writes an API URL by hand (ADR-0007). Biome's noRestrictedGlobals
// catches `fetch` while editing; this test also catches hand-written URLs and
// the retired `authFetch`, and fails CI for any of them.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, test } from "vitest";

const SRC = join(import.meta.dirname, ".");

/** Generated code, the mutator itself, and tests (which mock the network). */
function isExempt(path: string): boolean {
  return (
    path.startsWith("api/") ||
    path === "lib/api-client.ts" ||
    path === "routeTree.gen.ts" ||
    path.startsWith("test/") ||
    /\.test\.tsx?$/.test(path)
  );
}

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) return sourceFiles(full);
    return /\.(ts|tsx)$/.test(name) ? [full] : [];
  });
}

const BANNED: readonly { pattern: RegExp; what: string }[] = [
  { pattern: /(?<![\w.])fetch\s*\(/, what: "a direct fetch() call" },
  { pattern: /\bauthFetch\b/, what: "the retired authFetch" },
  { pattern: /["'`]\/api\//, what: "a hand-written /api/ URL" },
];

function findViolations(
  files: readonly { path: string; text: string }[],
) {
  return files.flatMap(({ path, text }) =>
    BANNED.filter(({ pattern }) => pattern.test(text)).map(
      ({ what }) => `${path}: ${what}`,
    ),
  );
}

describe("API access rule", () => {
  test("only the orval mutator calls fetch or names an API URL", () => {
    const files = sourceFiles(SRC)
      .map((full) => ({ path: relative(SRC, full), full }))
      .filter(({ path }) => !isExempt(path))
      .map(({ path, full }) => ({ path, text: readFileSync(full, "utf8") }));
    expect(files.length).toBeGreaterThan(10);
    expect(findViolations(files)).toEqual([]);
  });

  test("the rule catches each banned form", () => {
    const found = findViolations([
      { path: "a.ts", text: "await fetch('/x')" },
      { path: "b.ts", text: "authFetch({ url })" },
      { path: "c.ts", text: 'const url = "/api/v1/library";' },
      { path: "d.ts", text: "queryClient.fetchQuery(options); refetch();" },
    ]);
    expect(found).toEqual([
      "a.ts: a direct fetch() call",
      "b.ts: the retired authFetch",
      "c.ts: a hand-written /api/ URL",
    ]);
  });
});
