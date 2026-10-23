// Build-time half of the offline shell (public/sw.js). Vite copies sw.js
// verbatim, so after the bundle is written this plugin replaces its BUILD
// placeholder with the build's version and every emitted file under /assets/
// (the entry bundle, the CSS and each route chunk). Installing the worker then
// precaches all of them, so an offline reload works and an unvisited route's
// chunk is already there, and because sw.js now changes on every deploy the
// browser installs the new worker, whose activate step drops the old cache.

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import type { Plugin } from "vite";

/** The line in public/sw.js the build replaces; unbuilt, nothing is precached. */
export const BUILD_PLACEHOLDER =
  'const BUILD = { version: "dev", assets: [] };';

export interface ShellBuild {
  /** Names the worker's cache; changes whenever any emitted asset does. */
  version: string;
  /** Root-relative URLs of the emitted assets, sorted. */
  assets: string[];
}

/** The precache list and version for a build's output file names. */
export function shellBuild(fileNames: Iterable<string>): ShellBuild {
  const assets = [...fileNames]
    .filter((name) => name.startsWith("assets/") && !name.endsWith(".map"))
    .map((name) => `/${name}`)
    .sort();
  // Asset names are content-hashed, so their list fingerprints the build.
  const version = createHash("sha256")
    .update(assets.join("\n"))
    .digest("hex")
    .slice(0, 12);
  return { version, assets };
}

/** public/sw.js with the build's manifest in place of its placeholder. */
export function injectBuild(source: string, build: ShellBuild): string {
  if (!source.includes(BUILD_PLACEHOLDER)) {
    throw new Error(`sw.js no longer contains: ${BUILD_PLACEHOLDER}`);
  }
  return source.replace(
    BUILD_PLACEHOLDER,
    `const BUILD = ${JSON.stringify(build)};`,
  );
}

/** Writes the build's manifest into the copied dist/sw.js. */
export function precacheShell(): Plugin {
  let outDir = "dist";
  return {
    name: "stacks:precache-shell",
    apply: "build",
    configResolved(config) {
      outDir = resolve(config.root, config.build.outDir);
    },
    async writeBundle(_options, bundle) {
      const path = resolve(outDir, "sw.js");
      const source = await readFile(path, "utf8");
      await writeFile(
        path,
        injectBuild(source, shellBuild(Object.keys(bundle))),
      );
    },
  };
}
