import { useEffect, useRef } from "react";
import { Button } from "@/components/ui/button";

/**
 * The end of an infinite list: fetches the next page as it scrolls into
 * view, and offers a button for keyboards, screen readers and browsers
 * without IntersectionObserver. Renders nothing once the last page is in.
 */
export function LoadMore({
  hasNextPage,
  isFetchingNextPage,
  fetchNextPage,
  label = "Load more",
}: {
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchNextPage: () => unknown;
  label?: string;
}) {
  const sentinel = useRef<HTMLDivElement>(null);
  const state = useRef({ hasNextPage, isFetchingNextPage, fetchNextPage });
  state.current = { hasNextPage, isFetchingNextPage, fetchNextPage };

  useEffect(() => {
    const node = sentinel.current;
    if (!node || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(
      (entries) => {
        const { hasNextPage, isFetchingNextPage, fetchNextPage } =
          state.current;
        if (
          entries.some((e) => e.isIntersecting) &&
          hasNextPage &&
          !isFetchingNextPage
        ) {
          fetchNextPage();
        }
      },
      { rootMargin: "600px 0px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  return (
    <div ref={sentinel} className="flex justify-center py-6">
      {hasNextPage && (
        <Button
          variant="secondary"
          loading={isFetchingNextPage}
          onClick={() => fetchNextPage()}
        >
          {label}
        </Button>
      )}
    </div>
  );
}
