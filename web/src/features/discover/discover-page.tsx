import { useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ChevronRight, CloudOff, EyeOff, Sparkles } from "lucide-react";
import type { Feed, Kind, TitlePage } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import {
  useHiddenRefs,
  withoutHidden,
} from "@/features/blocklist/use-blocklist";
import { LoadMore } from "@/features/library/load-more";
import { RAILS, type Rail, railTitle } from "./feeds";
import { TitleCard } from "./title-card";
import { feedPagesOptions, useInView } from "./use-feed";

/** How many titles a rail shows before "See all". */
const RAIL_LENGTH = 20;

/** Discover: a rail per feed, each loading (and failing) on its own. */
export function DiscoverPage() {
  return (
    <>
      <PageHeader
        title="Discover"
        description="What's trending in movies, shows, anime and dramas."
        actions={
          <Link
            to="/blocklist"
            className="inline-flex items-center gap-1.5 text-sm font-medium text-fg-muted hover:text-fg"
          >
            <EyeOff aria-hidden className="size-4" />
            Not interested
          </Link>
        }
      />
      <div className="flex flex-col gap-8">
        {RAILS.map((rail) => (
          <FeedRail key={`${rail.feed}:${rail.kind ?? ""}`} rail={rail} />
        ))}
      </div>
    </>
  );
}

function railId(rail: Rail): string {
  return `rail-${rail.feed}-${rail.kind ?? "default"}`;
}

/** Why a page came back short: the providers that failed to answer. */
function DegradedNote({ page }: { page: TitlePage }) {
  if (page.degraded.length === 0) return null;
  const providers = [...new Set(page.degraded.map((d) => d.provider))];
  return (
    <p className="flex items-center gap-1.5 text-xs text-fg-muted">
      <CloudOff aria-hidden className="size-3.5 text-warning" />
      {providers.map((p) => p.toUpperCase()).join(" and ")} didn't answer, so
      some titles may be missing.
    </p>
  );
}

function FeedRail({ rail }: { rail: Rail }) {
  const [ref, inView] = useInView<HTMLElement>();
  const pages = useInfiniteQuery({
    ...feedPagesOptions(rail.feed, rail.kind),
    enabled: inView,
  });
  const hidden = useHiddenRefs();
  const headingId = railId(rail);
  const first = pages.data?.pages[0];
  const items = first
    ? withoutHidden(first.items, hidden).slice(0, RAIL_LENGTH)
    : [];

  return (
    <section
      ref={ref}
      aria-labelledby={headingId}
      className="flex flex-col gap-3"
    >
      <div className="flex items-baseline justify-between gap-4">
        <h2
          id={headingId}
          className="font-display text-lg font-semibold text-fg"
        >
          {rail.title}
        </h2>
        <Link
          to="/discover/$feed"
          params={{ feed: rail.feed }}
          search={{ kind: rail.kind }}
          aria-label={`See all ${rail.title}`}
          className="inline-flex items-center gap-0.5 text-sm font-medium text-accent hover:underline"
        >
          See all
          <ChevronRight aria-hidden className="size-4" />
        </Link>
      </div>
      {pages.isError ? (
        <ErrorState
          error={pages.error}
          onRetry={() => void pages.refetch()}
          className="py-6"
        />
      ) : !first ? (
        <div aria-hidden className="flex gap-3 overflow-hidden">
          {Array.from({ length: 8 }, (_, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
            <Skeleton key={i} className="aspect-[2/3] w-36 shrink-0" />
          ))}
        </div>
      ) : items.length === 0 ? (
        <>
          <DegradedNote page={first} />
          <p className="text-sm text-fg-muted">Nothing here right now.</p>
        </>
      ) : (
        <>
          <DegradedNote page={first} />
          <ul className="-mx-1 flex snap-x gap-3 overflow-x-auto px-1 pb-2">
            {items.map((title) => (
              <li key={title.ref} className="shrink-0 snap-start">
                <TitleCard title={title} />
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

/** One feed in full, as an infinitely scrolling grid. */
export function FeedPage({ feed, kind }: { feed: Feed; kind?: Kind }) {
  const pages = useInfiniteQuery(feedPagesOptions(feed, kind));
  const hidden = useHiddenRefs();
  const title = railTitle(feed, kind);

  let body: React.ReactNode;
  if (pages.isPending) {
    body = (
      <div
        aria-hidden
        className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6"
      >
        {Array.from({ length: 12 }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
          <Skeleton key={i} className="aspect-[2/3]" />
        ))}
      </div>
    );
  } else if (pages.isError) {
    body = (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  } else {
    // A title can appear on two pages when the provider's ranking shifts.
    const seen = new Set<string>();
    const items = withoutHidden(
      pages.data.pages.flatMap((page) => page.items),
      hidden,
    ).filter((item) => !seen.has(item.ref) && Boolean(seen.add(item.ref)));
    const last = pages.data.pages.at(-1);
    body =
      items.length === 0 ? (
        <EmptyState
          icon={<Sparkles />}
          title="Nothing here right now"
          description="The providers returned no titles for this feed."
        />
      ) : (
        <>
          {last && <DegradedNote page={last} />}
          <ul className="mt-3 grid grid-cols-2 justify-items-center gap-x-4 gap-y-6 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
            {items.map((item) => (
              <li key={item.ref}>
                <TitleCard title={item} />
              </li>
            ))}
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
      <nav aria-label="Breadcrumb" className="mb-2 text-sm">
        <Link to="/discover" className="text-fg-muted hover:text-fg">
          Discover
        </Link>
      </nav>
      <PageHeader title={title} />
      {body}
    </>
  );
}
