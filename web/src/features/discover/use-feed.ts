// A discovery feed as an infinite query, and the "is it on screen yet" signal
// that lets each Discover rail load only when it scrolls near.

import { infiniteQueryOptions } from "@tanstack/react-query";
import { type RefObject, useEffect, useRef, useState } from "react";
import { getFeed, getGetFeedQueryKey } from "@/api/catalog/catalog";
import type { Feed, Kind, TitlePage } from "@/api/schemas";

/**
 * Every page of `feed`. A rail and its see-all page share the query, and its
 * key extends the generated feed key, so invalidating that refreshes both.
 */
export function feedPagesOptions(feed: Feed, kind: Kind | undefined) {
  const params = kind ? { kind } : {};
  return infiniteQueryOptions({
    queryKey: [...getGetFeedQueryKey(feed, params), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      getFeed(feed, { ...params, cursor: pageParam }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: TitlePage) => last.next_cursor ?? undefined,
  });
}

/**
 * Whether the element has come within `margin` of the viewport; stays true
 * once it has. Without IntersectionObserver everything counts as in view.
 */
export function useInView<T extends Element>(
  margin = "400px",
): [RefObject<T | null>, boolean] {
  const ref = useRef<T>(null);
  const [inView, setInView] = useState(
    () => typeof IntersectionObserver === "undefined",
  );
  useEffect(() => {
    const node = ref.current;
    if (inView || !node || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) setInView(true);
      },
      { rootMargin: `${margin} 0px` },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [inView, margin]);
  return [ref, inView];
}
