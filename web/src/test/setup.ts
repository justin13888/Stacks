// Shared test setup: jest-dom matchers, and the browser APIs jsdom lacks that
// Radix and the app read (matchMedia, ResizeObserver, pointer capture,
// scrollIntoView).

import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";
import { clearToasts } from "@/components/ui/toast";

// `findBy*` and `waitFor` give up after this long. The default (1 s) is
// shorter than a full-app render takes under load, which failed tests that
// were only slow; vitest.config.ts bounds each test as a whole.
configure({ asyncUtilTimeout: 5_000 });

/** Media queries that currently match; tests set them with `setMediaQueries`. */
const matching = new Set<string>();
const mediaListeners = new Map<
  string,
  Set<(event: MediaQueryListEvent) => void>
>();

export function setMediaQueries(queries: readonly string[]): void {
  matching.clear();
  for (const query of queries) matching.add(query);
  for (const [query, listeners] of mediaListeners) {
    for (const listener of listeners) {
      listener({
        matches: matching.has(query),
        media: query,
      } as MediaQueryListEvent);
    }
  }
}

function matchMedia(query: string): MediaQueryList {
  const listeners = mediaListeners.get(query) ?? new Set();
  mediaListeners.set(query, listeners);
  return {
    get matches() {
      return matching.has(query);
    },
    media: query,
    onchange: null,
    addEventListener: (
      _type: string,
      listener: (event: MediaQueryListEvent) => void,
    ) => listeners.add(listener),
    removeEventListener: (
      _type: string,
      listener: (event: MediaQueryListEvent) => void,
    ) => listeners.delete(listener),
    addListener: (listener: (event: MediaQueryListEvent) => void) =>
      listeners.add(listener),
    removeListener: (listener: (event: MediaQueryListEvent) => void) =>
      listeners.delete(listener),
    dispatchEvent: () => false,
  } as unknown as MediaQueryList;
}

Object.defineProperty(window, "matchMedia", {
  configurable: true,
  value: matchMedia,
});

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub;

Element.prototype.scrollIntoView ??= () => {};
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};

afterEach(() => {
  cleanup();
  clearToasts();
  setMediaQueries([]);
  window.localStorage.clear();
  delete document.documentElement.dataset.theme;
});
