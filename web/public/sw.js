// Stacks offline shell.
//
// Caches the static app shell (the HTML entry, the fingerprinted bundle and
// the icons) so the installed app opens without a network. It never caches
// an API response: anything under /api/ and the health probes go straight to
// the network, so offline the app reports that it cannot reach the server
// rather than showing data that may be out of date.

const CACHE = "stacks-shell-v1";
const SHELL = [
  "/",
  "/manifest.webmanifest",
  "/icons/icon.svg",
  "/icons/icon-192.png",
  "/icons/icon-512.png",
];

/** Requests the worker never touches. */
function isPassThrough(url) {
  return (
    url.origin !== self.location.origin ||
    url.pathname.startsWith("/api/") ||
    url.pathname === "/healthz" ||
    url.pathname === "/readyz" ||
    url.pathname === "/sw.js"
  );
}

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(SHELL))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys.filter((key) => key !== CACHE).map((key) => caches.delete(key)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

/** Pages: the network first, so a deploy is picked up; the cached shell offline. */
async function navigate(request) {
  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(CACHE);
      await cache.put("/", response.clone());
    }
    return response;
  } catch (error) {
    const shell = await caches.match("/");
    if (shell) return shell;
    throw error;
  }
}

/** Fingerprinted assets never change, so the cache answers first. */
async function asset(request) {
  const cached = await caches.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) {
    const cache = await caches.open(CACHE);
    await cache.put(request, response.clone());
  }
  return response;
}

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (isPassThrough(url)) return;

  if (request.mode === "navigate") {
    event.respondWith(navigate(request));
  } else if (
    url.pathname.startsWith("/assets/") ||
    SHELL.includes(url.pathname)
  ) {
    event.respondWith(asset(request));
  }
});
