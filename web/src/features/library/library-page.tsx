import { useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  Columns3,
  LayoutGrid,
  Library as LibraryIcon,
  List,
  Search,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Category, Sort, Status } from "@/api/schemas";
import { useListTags } from "@/api/tags/tags";
import { PageHeader } from "@/components/layout/page";
import { CATEGORIES, CATEGORY_LABELS } from "@/components/search/categories";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { STATUSES } from "@/features/titles/library-controls";
import { useDebouncedValue } from "@/hooks/use-debounced-value";
import { BoardView } from "./board-view";
import { EntryCard } from "./entry-card";
import { ListView } from "./list-view";
import { LoadMore } from "./load-more";
import {
  LIBRARY_VIEWS,
  type LibrarySearch,
  type LibraryView,
  libraryParams,
} from "./search";
import { libraryPagesOptions } from "./use-library";

/** How long typing in the filter pauses before the URL and listing follow. */
export const FILTER_DEBOUNCE_MS = 300;

const ALL = "all";

const VIEW_LABELS: Record<LibraryView, string> = {
  grid: "Grid",
  list: "List",
  board: "Board",
};

const VIEW_ICONS: Record<LibraryView, typeof LayoutGrid> = {
  grid: LayoutGrid,
  list: List,
  board: Columns3,
};

const SORT_OPTIONS: { value: Sort; label: string }[] = [
  { value: "updated", label: "Recently updated" },
  { value: "added", label: "Recently added" },
  { value: "title", label: "Title" },
  { value: "score", label: "Score" },
];

export interface LibraryPageProps {
  search: LibrarySearch;
  /** Replaces some search params; `undefined` removes one. */
  onSearchChange: (changes: Partial<LibrarySearch>) => void;
}

/** Everything the user tracks, as a poster grid, a dense list or a status board. */
export function LibraryPage({ search, onSearchChange }: LibraryPageProps) {
  const view = search.view ?? "grid";
  return (
    <>
      <PageHeader
        title="Library"
        description="Everything you track, filtered and sorted your way."
        actions={
          <Tabs
            value={view}
            onValueChange={(next) =>
              onSearchChange({
                view: next === "grid" ? undefined : (next as LibraryView),
              })
            }
          >
            <TabsList aria-label="View">
              {LIBRARY_VIEWS.map((v) => {
                const Icon = VIEW_ICONS[v];
                return (
                  <TabsTrigger key={v} value={v} className="gap-1.5">
                    <Icon aria-hidden className="size-4" />
                    {VIEW_LABELS[v]}
                  </TabsTrigger>
                );
              })}
            </TabsList>
          </Tabs>
        }
      />
      <Filters
        search={search}
        onSearchChange={onSearchChange}
        showStatus={view !== "board"}
      />
      {view === "board" ? (
        <BoardView search={search} />
      ) : (
        <Listing search={search} view={view} onSearchChange={onSearchChange} />
      )}
    </>
  );
}

function Filters({
  search,
  onSearchChange,
  showStatus,
}: LibraryPageProps & { showStatus: boolean }) {
  const tags = useListTags({ limit: 200 });
  const [text, setText] = useState(search.q ?? "");
  // The URL changed on its own (Back, "Clear filters"): the field follows.
  const [urlQ, setUrlQ] = useState(search.q);
  if (urlQ !== search.q) {
    setUrlQ(search.q);
    if ((search.q ?? "") !== text.trim()) setText(search.q ?? "");
  }
  // Typing settles, then the URL (and so the listing) follows.
  const q = useDebouncedValue(text.trim(), FILTER_DEBOUNCE_MS);
  const latest = useRef({ search, onSearchChange });
  latest.current = { search, onSearchChange };
  useEffect(() => {
    const { search, onSearchChange } = latest.current;
    if ((search.q ?? "") !== q) onSearchChange({ q: q || undefined });
  }, [q]);

  const tagOptions = tags.data?.items ?? [];
  return (
    <search
      aria-label="Filter the library"
      className="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-3 lg:flex lg:flex-wrap lg:items-end"
    >
      <div className="relative col-span-2 sm:col-span-3 lg:w-64">
        <Search
          aria-hidden
          className="pointer-events-none absolute top-2.5 left-3 size-5 text-fg-subtle"
        />
        <Input
          aria-label="Filter by title"
          type="search"
          placeholder="Filter by title…"
          value={text}
          onChange={(event) => setText(event.target.value)}
          className="pl-10"
        />
      </div>
      {showStatus && (
        <Select<Status | typeof ALL>
          label="Status"
          value={search.status ?? ALL}
          options={[
            { value: ALL, label: "Any status" },
            ...STATUSES.map((s) => ({ value: s, label: STATUS_LABELS[s] })),
          ]}
          onValueChange={(value) =>
            onSearchChange({ status: value === ALL ? undefined : value })
          }
          className="lg:w-40"
        />
      )}
      <Select<Category | typeof ALL>
        label="Category"
        value={search.category ?? ALL}
        options={[
          { value: ALL, label: "Every category" },
          ...CATEGORIES.map((c) => ({ value: c, label: CATEGORY_LABELS[c] })),
        ]}
        onValueChange={(value) =>
          onSearchChange({ category: value === ALL ? undefined : value })
        }
        className="lg:w-40"
      />
      <Select<"yes" | "no" | typeof ALL>
        label="Favourites"
        value={
          search.favourite === undefined ? ALL : search.favourite ? "yes" : "no"
        }
        options={[
          { value: ALL, label: "Everything" },
          { value: "yes", label: "Favourites only" },
          { value: "no", label: "Not favourites" },
        ]}
        onValueChange={(value) =>
          onSearchChange({
            favourite: value === ALL ? undefined : value === "yes",
          })
        }
        className="lg:w-40"
      />
      {tagOptions.length > 0 && (
        <Select<string>
          label="Tag"
          value={search.tag ?? ALL}
          options={[
            { value: ALL, label: "Any tag" },
            ...tagOptions.map((t) => ({ value: t.id, label: t.name })),
          ]}
          onValueChange={(value) =>
            onSearchChange({ tag: value === ALL ? undefined : value })
          }
          className="lg:w-40"
        />
      )}
      <Select<Sort>
        label="Sort"
        value={search.sort ?? "updated"}
        options={SORT_OPTIONS}
        onValueChange={(value) =>
          onSearchChange({ sort: value === "updated" ? undefined : value })
        }
        className="lg:w-44"
      />
    </search>
  );
}

function isFiltered(search: LibrarySearch): boolean {
  return Boolean(
    search.status ||
      search.category ||
      search.favourite !== undefined ||
      search.tag ||
      search.q,
  );
}

function Listing({
  search,
  view,
  onSearchChange,
}: LibraryPageProps & { view: "grid" | "list" }) {
  const pages = useInfiniteQuery(libraryPagesOptions(libraryParams(search)));

  if (pages.isPending) return <ListingSkeleton view={view} />;
  if (pages.isError) {
    return (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  }
  const entries = pages.data.pages.flatMap((page) => page.items);
  if (entries.length === 0) {
    return isFiltered(search) ? (
      <EmptyState
        icon={<Search />}
        title="Nothing matches these filters"
        description="Try another status or category, or clear the filters."
        action={
          <Link
            to="/library"
            search={{ view: search.view }}
            className="text-sm font-medium text-accent hover:underline"
          >
            Clear filters
          </Link>
        }
      />
    ) : (
      <EmptyState
        icon={<LibraryIcon />}
        title="Your library is empty"
        description="Press ⌘K or Ctrl K to find a movie, show or anime, then add it with a status."
      />
    );
  }

  return (
    <>
      {view === "grid" ? (
        <ul className="grid grid-cols-2 gap-x-4 gap-y-6 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
          {entries.map((entry) => (
            <li key={entry.title.ref}>
              <EntryCard entry={entry} />
            </li>
          ))}
        </ul>
      ) : (
        <ListView
          entries={entries}
          sort={search.sort ?? "updated"}
          onSort={(sort) =>
            onSearchChange({ sort: sort === "updated" ? undefined : sort })
          }
        />
      )}
      <LoadMore
        hasNextPage={pages.hasNextPage}
        isFetchingNextPage={pages.isFetchingNextPage}
        fetchNextPage={() => pages.fetchNextPage()}
      />
    </>
  );
}

function ListingSkeleton({ view }: { view: "grid" | "list" }) {
  if (view === "list") {
    return (
      <div aria-hidden className="flex flex-col gap-2">
        {Array.from({ length: 8 }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
          <Skeleton key={i} className="h-14" />
        ))}
      </div>
    );
  }
  return (
    <div
      aria-hidden
      className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6"
    >
      {Array.from({ length: 12 }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: placeholders
        <Skeleton key={i} className="aspect-[2/3]" />
      ))}
    </div>
  );
}
