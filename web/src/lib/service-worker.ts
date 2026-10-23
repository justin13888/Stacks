// Registers the offline shell (public/sw.js) in production builds. The worker
// caches only the static app shell, never an API response, so offline the app
// opens and says it cannot reach the server instead of showing stale data.

export function registerServiceWorker(): void {
  if (!import.meta.env.PROD) return;
  if (!("serviceWorker" in navigator)) return;
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("/sw.js", { scope: "/" }).catch(() => {
      // An unsupported or blocked worker only costs the offline shell.
    });
  });
}
