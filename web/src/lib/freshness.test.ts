import { QueryClient } from "@tanstack/react-query";
import { describe, expect, test } from "vitest";
import { FreshnessTracker, isStaleResponse } from "./freshness";

describe("isStaleResponse", () => {
  test("a title served from the snapshot is stale", () => {
    expect(isStaleResponse({ freshness: { stale: true } })).toBe(true);
    expect(isStaleResponse({ freshness: { stale: false } })).toBe(false);
  });

  test("a listing with failed provider calls is stale", () => {
    expect(
      isStaleResponse({ items: [], degraded: [{ provider: "tmdb" }] }),
    ).toBe(true);
    expect(isStaleResponse({ items: [], degraded: [] })).toBe(false);
  });

  test("anything else is not", () => {
    expect(isStaleResponse(undefined)).toBe(false);
    expect(isStaleResponse("text")).toBe(false);
    expect(isStaleResponse({ items: [] })).toBe(false);
  });
});

describe("FreshnessTracker", () => {
  test("follows the query cache as stale data arrives and leaves", () => {
    const client = new QueryClient();
    const tracker = new FreshnessTracker();
    tracker.watch(client.getQueryCache());
    let notified = 0;
    tracker.subscribe(() => {
      notified += 1;
    });

    client.setQueryData(["title", 1], { freshness: { stale: true } });
    expect(tracker.hasStale()).toBe(true);
    client.setQueryData(["title", 2], { freshness: { stale: true } });
    client.setQueryData(["title", 1], { freshness: { stale: false } });
    expect(tracker.hasStale()).toBe(true);
    client.removeQueries({ queryKey: ["title", 2] });
    expect(tracker.hasStale()).toBe(false);
    // Only the flips notify.
    expect(notified).toBe(2);
  });
});
