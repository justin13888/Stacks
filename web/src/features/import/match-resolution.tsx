import { keepPreviousData, useQueryClient } from "@tanstack/react-query";
import { CircleCheck, Search } from "lucide-react";
import { useState } from "react";
import { useSearchTitles } from "@/api/catalog/catalog";
import {
  getGetImportQueryKey,
  getListImportsQueryKey,
  useResolveImportItem,
} from "@/api/imports/imports";
import type {
  ImportDetail,
  ImportItem,
  TitleRef,
  TitleSummary,
} from "@/api/schemas";
import { SEARCH_DEBOUNCE_MS } from "@/components/search/command-palette";
import { CATEGORY_NAMES } from "@/components/search/categories";
import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { Poster } from "@/components/ui/poster";
import { toast } from "@/components/ui/toast";
import { invalidateTracking } from "@/features/library/entry";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { reportFailure } from "@/features/titles/use-tracking";
import { useDebouncedValue } from "@/hooks/use-debounced-value";
import { cn } from "@/lib/cn";
import { percent, unmatchedRows } from "./sources";

/**
 * Step 4: each row no title matched with certainty, with what the file says
 * about it and the titles it might be. A row is resolved by accepting a
 * candidate or a title found by search (`PATCH /imports/{id}/items/{n}`),
 * or set aside with Skip: the API has no skip yet (#39), so a skipped row
 * stays unmatched on the server and can be resolved later from history.
 */
export function MatchResolution({ detail }: { detail: ImportDetail }) {
  const [skipped, setSkipped] = useState<ReadonlySet<number>>(new Set());
  const unmatched = unmatchedRows(detail.items);
  const open = unmatched.filter((item) => !skipped.has(item.row));
  if (unmatched.length === 0) {
    return (
      <p className="flex items-center gap-2 text-sm text-fg">
        <CircleCheck aria-hidden className="size-4 text-success" />
        Every row found its title.
      </p>
    );
  }
  return (
    <section aria-label="Resolve matches" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="font-semibold text-fg">
          {open.length} {open.length === 1 ? "row needs" : "rows need"} a title
        </h3>
        {skipped.size > 0 && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setSkipped(new Set())}
          >
            Show {skipped.size} skipped
          </Button>
        )}
      </div>
      <ul className="flex flex-col gap-3">
        {open.map((item) => (
          <ResolveRow
            key={item.row}
            importId={detail.id}
            item={item}
            onSkip={() =>
              setSkipped((current) => new Set(current).add(item.row))
            }
          />
        ))}
      </ul>
    </section>
  );
}

function RawValues({ item }: { item: ImportItem }) {
  const source = item.source;
  const values: [string, string | number | null | undefined][] = [
    ["Title", source?.title],
    ["Id", source?.source_id],
    ["Kind", source?.kind],
    ["Year", source?.year],
    ["Episodes", source?.episodes],
  ];
  return (
    <dl className="flex flex-wrap gap-x-4 gap-y-1 text-xs">
      {values
        .filter(([, value]) => value != null && value !== "")
        .map(([label, value]) => (
          <div key={label} className="flex gap-1">
            <dt className="text-fg-muted">{label}</dt>
            <dd className="text-fg">{value}</dd>
          </div>
        ))}
    </dl>
  );
}

function ResolveRow({
  importId,
  item,
  onSkip,
}: {
  importId: string;
  item: ImportItem;
  onSkip: () => void;
}) {
  const queryClient = useQueryClient();
  // Search opens on request: opening it for every row without candidates
  // would send one catalog search per row at once.
  const [searching, setSearching] = useState(false);
  const label = item.source?.title ?? `Row ${item.row}`;
  const resolve = useResolveImportItem({
    mutation: {
      onSuccess: (resolved) => {
        queryClient.setQueryData<ImportDetail>(
          getGetImportQueryKey(importId),
          (current) =>
            current && {
              ...current,
              items: current.items.map((i) =>
                i.row === resolved.row ? resolved : i,
              ),
            },
        );
        toast({
          tone: "success",
          title: "Matched",
          description: resolved.title?.title ?? label,
        });
      },
      onError: reportFailure,
      onSettled: () =>
        Promise.all([
          queryClient.invalidateQueries({
            queryKey: getGetImportQueryKey(importId),
          }),
          queryClient.invalidateQueries({ queryKey: getListImportsQueryKey() }),
          invalidateTracking(queryClient),
        ]),
    },
  });
  function choose(title: TitleRef) {
    resolve.mutate({ id: importId, row: item.row, data: { title } });
  }
  return (
    <li
      aria-label={label}
      className="flex flex-col gap-3 rounded-lg border border-line p-3"
    >
      <div className="flex flex-col gap-1">
        <span className="font-medium text-fg">{label}</span>
        <RawValues item={item} />
        {item.error && (
          <span className="text-xs text-fg-muted">{item.error}</span>
        )}
      </div>
      {item.candidates.length > 0 && (
        <ul
          aria-label={`Candidates for ${label}`}
          className="flex flex-col gap-2"
        >
          {item.candidates.map((candidate) => (
            <li
              key={candidate.title}
              className="flex flex-wrap items-center gap-2 rounded-md bg-surface-sunken px-3 py-2 text-sm"
            >
              <span className="min-w-0 flex-1">
                <span className="font-medium text-fg">{candidate.name}</span>{" "}
                <span className="text-xs text-fg-muted">
                  {[
                    candidate.year,
                    candidate.kind === "movie" ? "Movie" : "Show",
                  ]
                    .filter(Boolean)
                    .join(" · ")}{" "}
                  · {percent(candidate.confidence)} match
                </span>
              </span>
              <Button
                size="sm"
                aria-label={`Use ${candidate.name} for ${label}`}
                loading={
                  resolve.isPending &&
                  resolve.variables?.data.title === candidate.title
                }
                onClick={() => choose(candidate.title)}
              >
                Use this
              </Button>
            </li>
          ))}
        </ul>
      )}
      {searching && (
        <TitleSearch
          label={`Search for ${label}`}
          initial={item.source?.title ?? ""}
          onSelect={(title) => choose(title.ref)}
        />
      )}
      <div className="flex flex-wrap gap-2">
        {!searching && (
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setSearching(true)}
          >
            <Search aria-hidden className="size-4" />
            Search
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          aria-label={`Skip ${label}`}
          onClick={onSkip}
        >
          Skip
        </Button>
      </div>
    </li>
  );
}

/** The palette's title search, as a field that picks one title. */
function TitleSearch({
  label,
  initial,
  onSelect,
}: {
  label: string;
  initial: string;
  onSelect: (title: TitleSummary) => void;
}) {
  const { title_language } = usePreferences();
  const [text, setText] = useState(initial);
  const q = useDebouncedValue(text.trim(), SEARCH_DEBOUNCE_MS);
  const search = useSearchTitles(
    { q },
    { query: { enabled: q.length > 0, placeholderData: keepPreviousData } },
  );
  const items = q.length > 0 ? (search.data?.items ?? []) : [];
  let status: React.ReactNode = null;
  if (q.length === 0) status = "Type a title to search.";
  else if (search.isPending) status = "Searching…";
  else if (search.isError) status = "Search failed. Try again.";
  else if (items.length === 0) status = "No titles found.";
  return (
    <Combobox
      label={label}
      hideLabel
      placeholder="Search titles"
      inputValue={text}
      onInputValueChange={setText}
      items={items}
      getKey={(title) => title.ref}
      getLabel={(title) => displayTitle(title, title_language)}
      onSelect={onSelect}
      className="max-h-80 rounded-md border border-line"
      status={<p className="px-3 py-2 text-sm text-fg-muted">{status}</p>}
      renderItem={(title, { active }) => {
        const name = displayTitle(title, title_language);
        return (
          <span
            className={cn(
              "flex items-center gap-3 px-3 py-2",
              active && "bg-accent-soft",
            )}
          >
            <Poster src={title.poster_url} title={name} size="xs" />
            <span className="flex min-w-0 flex-col">
              <span className="truncate text-sm text-fg">{name}</span>
              <span className="text-xs text-fg-muted">
                {[title.year, CATEGORY_NAMES[title.category]]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            </span>
          </span>
        );
      }}
    />
  );
}
