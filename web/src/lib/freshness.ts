// Whether an API response was served from the stored catalog snapshot.
//
// When a metadata provider fails, the API answers from its snapshot and says
// so: a title, its seasons and a season carry `freshness: { stale: true }`
// (TitleDetail, Seasons, SeasonDetail), and a listing assembled from several
// provider calls lists the ones that failed in `degraded` (TitlePage,
// ListItemPage). The shell shows one banner while any cached response says
// either.

import type { QueryCache } from "@tanstack/react-query";
import { useSyncExternalStore } from "react";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isStaleFreshness(value: unknown): boolean {
  return isRecord(value) && value.stale === true;
}

/** True when `data` (any response body) reports stale or degraded data. */
export function isStaleResponse(data: unknown): boolean {
  if (!isRecord(data)) return false;
  if (isStaleFreshness(data.freshness)) return true;
  return Array.isArray(data.degraded) && data.degraded.length > 0;
}

/**
 * Tracks which cached queries currently hold stale data. `watch` subscribes to
 * a query cache; `useHasStaleData` re-renders the banner when the answer flips.
 */
export class FreshnessTracker {
  private readonly stale = new Set<string>();
  private readonly listeners = new Set<() => void>();
  private snapshot = false;

  watch(cache: QueryCache): () => void {
    return cache.subscribe((event) => {
      const hash = event.query.queryHash;
      if (event.type === "removed") {
        this.set(hash, false);
      } else if (event.type === "updated" && event.action.type === "success") {
        this.set(hash, isStaleResponse(event.action.data));
      }
    });
  }

  hasStale = (): boolean => this.snapshot;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  private set(hash: string, stale: boolean): void {
    if (stale) this.stale.add(hash);
    else this.stale.delete(hash);
    const next = this.stale.size > 0;
    if (next === this.snapshot) return;
    this.snapshot = next;
    for (const listener of this.listeners) listener();
  }
}

export function useHasStaleData(tracker: FreshnessTracker): boolean {
  return useSyncExternalStore(tracker.subscribe, tracker.hasStale, () => false);
}
