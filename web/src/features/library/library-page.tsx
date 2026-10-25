import { keepPreviousData, useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  Columns3,
  LayoutGrid,
  Library as LibraryIcon,
  List,
  Search,
  Tag as TagIcon,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Category, Sort, Status } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { CATEGORIES, CATEGORY_LABELS } from "@/components/search/categories";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Input } from "@/components/ui/input";
import { LoadMore } from "@/components/ui/load-more";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import { TagAssignDialog, type TaggedTitle } from "@/features/tags/tag-assign";
import { TagFilter } from "@/features/tags/tag-filter";
import { STATUSES } from "@/features/titles/library-controls";
import { useDebouncedValue } from "@/hooks/use-debounced-value";
import { cn } from "@/lib/cn";
import { BoardView } from "./board-view";
import { EntryCard } from "./entry-card";
import { ListView } from "./list-view";
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
  { value: "progress", label: "Progress" },
  { value: "started", label: "Start date" },
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
          // A view switch, not tabs: each view is the same listing.
          <fieldset className="inline-flex items-center gap-1 rounded-lg bg-surface-sunken p-1">
            <legend className="sr-only">View</legend>
            {LIBRARY_VIEWS.map((v) => {
              const Icon = VIEW_ICONS[v];
              const active = v === view;
              return (
                <button
                  key={v}
                  type="button"
                  aria-pressed={active}
                  onClick={() =>
                    onSearchChange({ view: v === "grid" ? undefined : v })
                  }
                  className={cn(
                    "inline-flex h-8 items-center justify-center gap-1.5 rounded-md px-3 text-sm font-medium text-fg-muted",
                    "transition-colors duration-(--duration-fast) hover:text-fg focus-visible:outline-2 focus-visible:outline-ring",
                    active && "bg-surface text-fg shadow-raised",
                  )}
                >
                  <Icon aria-hidden className="size-4" />
                  {VIEW_LABELS[v]}
                </button>
              );
            })}
          </fieldset>
        }
      />
      <Filters
        search={search}
        onSearchChange={onSearchChange}
        showStatus={view !== "board"}
      />
      <TagFilter
        value={search.tag}
        onChange={(tag) => onSearchChange({ tag })}
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

  return (
    <section
      aria-label="Filter the library"
      className="mb-3 grid grid-cols-2 gap-3 sm:grid-cols-3 lg:flex lg:flex-wrap lg:items-end"
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
      <Select<Sort>
        label="Sort"
        value={search.sort ?? "updated"}
        options={SORT_OPTIONS}
        onValueChange={(value) =>
          onSearchChange({ sort: value === "updated" ? undefined : value })
        }
        className="lg:w-44"
      />
    </section>
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
  const pages = useInfiniteQuery({
    ...libraryPagesOptions(libraryParams(search)),
    // A new filter keeps the old results on screen until its own arrive.
    placeholderData: keepPreviousData,
  });
  // The list view's selection, for tagging in bulk.
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());

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
        <>
          <BulkBar
            selected={entries
              .filter((e) => selected.has(e.title.ref))
              .map((e) => ({ ref: e.title.ref, tags: e.tags }))}
            onClear={() => setSelected(new Set())}
          />
          <ListView
            entries={entries}
            sort={search.sort ?? "updated"}
            onSort={(sort) =>
              onSearchChange({ sort: sort === "updated" ? undefined : sort })
            }
            selected={selected}
            onSelectedChange={setSelected}
          />
        </>
      )}
      <LoadMore
        hasNextPage={pages.hasNextPage}
        isFetchingNextPage={pages.isFetchingNextPage}
        fetchNextPage={() => pages.fetchNextPage()}
      />
    </>
  );
}

/** The list view's bulk actions, for the titles selected in it. */
function BulkBar({
  selected,
  onClear,
}: {
  selected: readonly TaggedTitle[];
  onClear: () => void;
}) {
  const [tagging, setTagging] = useState(false);
  const count = selected.length;
  const subject = count === 1 ? "1 title" : `${count} titles`;
  return (
    <div
      role="toolbar"
      aria-label="Selected titles"
      className="mb-3 flex min-h-10 flex-wrap items-center gap-2 text-sm text-fg-muted"
    >
      {count === 0 ? (
        <span>Select titles to tag them together.</span>
      ) : (
        <>
          <span className="font-medium text-fg">{subject} selected</span>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setTagging(true)}
          >
            <TagIcon aria-hidden className="size-4" />
            Tag selected
          </Button>
          <Button variant="ghost" size="sm" onClick={onClear}>
            Clear selection
          </Button>
        </>
      )}
      <TagAssignDialog
        titles={selected}
        subject={subject}
        open={tagging}
        onOpenChange={setTagging}
      />
    </div>
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
