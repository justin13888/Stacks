// The PWA: the manifest is installable, and the service worker serves the
// static shell offline without ever caching an API response.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";
import {
  BUILD_PLACEHOLDER,
  injectBuild,
  type ShellBuild,
  shellBuild,
} from "../../scripts/sw-precache";

const PUBLIC = join(import.meta.dirname, "../../public");

/** Width and height from a PNG's IHDR chunk. */
function pngSize(path: string): [number, number] {
  const bytes = readFileSync(path);
  expect(bytes.subarray(1, 4).toString("ascii")).toBe("PNG");
  return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
}

describe("web app manifest", () => {
  const manifest = JSON.parse(
    readFileSync(join(PUBLIC, "manifest.webmanifest"), "utf8"),
  );

  test("has what browsers require to install it", () => {
    expect(manifest).toMatchObject({
      name: "Stacks",
      short_name: "Stacks",
      start_url: "/",
      scope: "/",
      display: "standalone",
    });
    expect(manifest.theme_color).toMatch(/^#[0-9a-f]{6}$/i);
    expect(manifest.background_color).toMatch(/^#[0-9a-f]{6}$/i);
  });

  test("ships 192 and 512 pixel PNG icons, and a maskable one, that exist", () => {
    const icons = manifest.icons as {
      src: string;
      sizes: string;
      type: string;
      purpose: string;
    }[];
    for (const icon of icons) {
      const path = join(PUBLIC, icon.src);
      if (icon.type === "image/png") {
        expect(pngSize(path).join("x")).toBe(icon.sizes);
      } else {
        expect(readFileSync(path, "utf8")).toContain("<svg");
      }
    }
    const png = icons.filter((i) => i.type === "image/png");
    expect(png.map((i) => i.sizes)).toEqual(
      expect.arrayContaining(["192x192", "512x512"]),
    );
    expect(icons.some((i) => i.purpose === "maskable")).toBe(true);
  });

  test("index.html links it", () => {
    const html = readFileSync(
      join(import.meta.dirname, "../../index.html"),
      "utf8",
    );
    expect(html).toContain(
      '<link rel="manifest" href="/manifest.webmanifest" />',
    );
  });
});

const BUILD = shellBuild([
  "index.html",
  "assets/index-abc.js",
  "assets/index-abc.js.map",
  "assets/index-def.css",
  "assets/library-123.js",
]);

/** Runs public/sw.js, as the build writes it, against a fake worker scope and cache. */
function loadWorker(build: ShellBuild = BUILD) {
  const listeners = new Map<string, (event: unknown) => void>();
  const stored = new Map<string, Response>();
  const cache = {
    addAll: vi.fn(async () => {}),
    put: vi.fn(async (key: Request | string, response: Response) => {
      stored.set(
        typeof key === "string" ? key : new URL(key.url).pathname,
        response,
      );
    }),
  };
  const caches = {
    open: vi.fn(async () => cache),
    match: vi.fn(async (key: Request | string) =>
      stored.get(typeof key === "string" ? key : new URL(key.url).pathname),
    ),
    keys: vi.fn(async () => [
      "stacks-shell-previous",
      `stacks-shell-${build.version}`,
    ]),
    delete: vi.fn(async () => true),
  };
  const network = vi.fn<(request: Request) => Promise<Response>>();
  const self = {
    location: { origin: "https://stacks.example" },
    addEventListener: (type: string, listener: (event: unknown) => void) =>
      listeners.set(type, listener),
    skipWaiting: vi.fn(),
    clients: { claim: vi.fn() },
  };
  const source = injectBuild(
    readFileSync(join(PUBLIC, "sw.js"), "utf8"),
    build,
  );
  new Function("self", "caches", "fetch", source)(self, caches, network);

  /** Dispatches a fetch; resolves to the worker's response, or null if it let it pass. */
  async function dispatch(
    path: string,
    init: { method?: string; mode?: string } = {},
  ) {
    // Assigned inside the listener, which TypeScript cannot see.
    let responded = null as Promise<Response> | null;
    const request = {
      url: `https://stacks.example${path}`,
      method: init.method ?? "GET",
      mode: init.mode ?? "cors",
    } as Request;
    listeners.get("fetch")?.({
      request,
      respondWith: (response: Promise<Response>) => {
        responded = response;
      },
    });
    return responded === null ? null : await responded;
  }

  return { listeners, cache, caches, stored, network, dispatch };
}

describe("service worker", () => {
  test("installing precaches the shell and every built asset, in this build's cache", async () => {
    const { listeners, cache, caches } = loadWorker();
    let done: Promise<unknown> = Promise.resolve();
    listeners.get("install")?.({
      waitUntil: (p: Promise<unknown>) => (done = p),
    });
    await done;
    expect(caches.open).toHaveBeenCalledWith(`stacks-shell-${BUILD.version}`);
    expect(cache.addAll).toHaveBeenCalledWith(
      expect.arrayContaining([
        "/",
        "/manifest.webmanifest",
        "/assets/index-abc.js",
        "/assets/index-def.css",
        "/assets/library-123.js",
      ]),
    );
  });

  test("activating drops every older deploy's cache", async () => {
    const { listeners, caches } = loadWorker();
    let done: Promise<unknown> = Promise.resolve();
    listeners.get("activate")?.({
      waitUntil: (p: Promise<unknown>) => (done = p),
    });
    await done;
    expect(caches.delete).toHaveBeenCalledWith("stacks-shell-previous");
    expect(caches.delete).not.toHaveBeenCalledWith(
      `stacks-shell-${BUILD.version}`,
    );
  });

  test("API calls and health probes are never intercepted or cached", async () => {
    const { dispatch, cache } = loadWorker();
    expect(await dispatch("/api/v1/library")).toBeNull();
    expect(await dispatch("/api/v1/sessions/current")).toBeNull();
    expect(await dispatch("/healthz")).toBeNull();
    expect(await dispatch("/api/v1/library", { method: "PUT" })).toBeNull();
    expect(cache.put).not.toHaveBeenCalled();
  });

  test("offline, a page navigation gets the cached shell", async () => {
    const { dispatch, network, stored } = loadWorker();
    const shell = new Response("<!doctype html>");
    stored.set("/", shell);
    network.mockRejectedValue(new TypeError("offline"));
    expect(await dispatch("/library", { mode: "navigate" })).toBe(shell);
  });

  test("online, a navigation refreshes the cached shell", async () => {
    const { dispatch, network, cache } = loadWorker();
    network.mockResolvedValue(new Response("<!doctype html>", { status: 200 }));
    const response = await dispatch("/", { mode: "navigate" });
    expect(response?.status).toBe(200);
    expect(cache.put).toHaveBeenCalledWith("/", expect.anything());
  });

  test("fingerprinted assets are served from the cache first", async () => {
    const { dispatch, network, stored } = loadWorker();
    const cached = new Response("js");
    stored.set("/assets/index-abc.js", cached);
    expect(await dispatch("/assets/index-abc.js")).toBe(cached);
    expect(network).not.toHaveBeenCalled();
  });
});

describe("service worker build manifest", () => {
  test("lists every emitted asset but source maps, and nothing outside /assets/", () => {
    expect(BUILD.assets).toEqual([
      "/assets/index-abc.js",
      "/assets/index-def.css",
      "/assets/library-123.js",
    ]);
  });

  test("a deploy that changes any asset gets a new cache version", () => {
    const next = shellBuild(["assets/index-abd.js", "assets/index-def.css"]);
    expect(next.version).not.toBe(BUILD.version);
    expect(next.version).toMatch(/^[0-9a-f]{12}$/);
    expect(shellBuild([...BUILD.assets].map((a) => a.slice(1)))).toEqual(BUILD);
  });

  test("public/sw.js carries the placeholder the build replaces", () => {
    const source = readFileSync(join(PUBLIC, "sw.js"), "utf8");
    expect(source).toContain(BUILD_PLACEHOLDER);
    expect(injectBuild(source, BUILD)).toContain(
      `const BUILD = ${JSON.stringify(BUILD)};`,
    );
    expect(() => injectBuild("const BUILD = {};", BUILD)).toThrow();
  });
});
