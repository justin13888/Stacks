import { infiniteQueryOptions, useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { EyeOff } from "lucide-react";
import { getListBlockedQueryKey, listBlocked } from "@/api/blocklist/blocklist";
import type { PageBlockedTitle } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { CATEGORY_NAMES } from "@/components/search/categories";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Poster } from "@/components/ui/poster";
import { Skeleton } from "@/components/ui/skeleton";
import { formatInstant } from "@/features/calendar/dates";
import { LoadMore } from "@/features/library/load-more";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { useUnblock } from "./use-blocklist";

function blockedPagesOptions() {
  return infiniteQueryOptions({
    queryKey: [...getListBlockedQueryKey(), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listBlocked({ cursor: pageParam, limit: 50 }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageBlockedTitle) => last.next_cursor ?? undefined,
  });
}

/** The titles hidden with "Not interested", each with an unblock. */
export function BlocklistPage() {
  const { title_language } = usePreferences();
  const pages = useInfiniteQuery(blockedPagesOptions());
  const unblock = useUnblock();

  let body: React.ReactNode;
  if (pages.isPending) {
    body = (
      <div aria-hidden className="flex flex-col gap-2">
        {Array.from({ length: 5 }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
          <Skeleton key={i} className="h-16" />
        ))}
      </div>
    );
  } else if (pages.isError) {
    body = (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  } else {
    const items = pages.data.pages.flatMap((page) => page.items);
    body =
      items.length === 0 ? (
        <EmptyState
          icon={<EyeOff />}
          title="Nothing hidden"
          description="Choose “Not interested” on a Discover card to keep a title out of your feeds and recommendations."
        />
      ) : (
        <>
          <ul className="flex flex-col divide-y divide-line rounded-xl border border-line bg-surface">
            {items.map(({ title, blocked_at }) => {
              const name = displayTitle(title, title_language);
              const pending =
                unblock.isPending && unblock.variables?.ref === title.ref;
              return (
                <li key={title.ref} className="flex items-center gap-3 p-3">
                  <Poster src={title.poster_url} title={name} size="xs" />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <Link
                      to="/titles/$ref"
                      params={{ ref: title.ref }}
                      className="truncate font-medium text-fg hover:underline"
                    >
                      {name}
                    </Link>
                    <span className="text-xs text-fg-muted">
                      {[title.year, CATEGORY_NAMES[title.category]]
                        .filter(Boolean)
                        .join(" · ")}
                      {" · hidden "}
                      {formatInstant(blocked_at)}
                    </span>
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    loading={pending}
                    aria-label={`Unblock ${name}`}
                    onClick={() => unblock.mutate(title)}
                  >
                    Unblock
                  </Button>
                </li>
              );
            })}
          </ul>
          <LoadMore
            hasNextPage={pages.hasNextPage}
            isFetchingNextPage={pages.isFetchingNextPage}
            fetchNextPage={() => pages.fetchNextPage()}
          />
        </>
      );
  }

  return (
    <>
      <PageHeader
        title="Not interested"
        description="Titles you hid. They stay out of Discover, search and recommendations until you unblock them."
      />
      {body}
    </>
  );
}
