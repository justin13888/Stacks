import { keepPreviousData, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
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
  ResolvedImportItem,
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
import { percent, skippedRows, unmatchedRows } from "./sources";

/**
 * Step 4: each row no title matched with certainty, with what the file says
 * about it and the titles it might be. A row is resolved by accepting a
 * candidate or a title found by search, or set aside with Skip; both are
 * `PATCH /imports/{id}/items/{n}`, so the import's counts move with them. A
 * skipped row stays out of the library and can still be matched later.
 */
export function MatchResolution({ detail }: { detail: ImportDetail }) {
  const [showSkipped, setShowSkipped] = useState(false);
  const unmatched = unmatchedRows(detail.items);
  const skipped = skippedRows(detail.items);
  if (unmatched.length === 0 && skipped.length === 0) {
    return (
      <p className="flex items-center gap-2 text-sm text-fg">
        <CircleCheck aria-hidden className="size-4 text-success" />
        Every row found its title.
      </p>
    );
  }
  const shown = showSkipped
    ? [...unmatched, ...skipped].sort((a, b) => a.row - b.row)
    : unmatched;
  return (
    <section aria-label="Resolve matches" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="font-semibold text-fg">
          {unmatched.length === 0
            ? "No rows need a title"
            : `${unmatched.length} ${unmatched.length === 1 ? "row needs" : "rows need"} a title`}
        </h3>
        {skipped.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            aria-expanded={showSkipped}
            onClick={() => setShowSkipped((shown) => !shown)}
          >
            {showSkipped ? "Hide skipped" : `Show ${skipped.length} skipped`}
          </Button>
        )}
      </div>
      {skipped.length > 0 && (
        <p className="text-sm text-fg-muted">
          Skipped rows stay out of your library. Show them to match one later.
        </p>
      )}
      <ul className="flex flex-col gap-3">
        {shown.map((item) => (
          <ResolveRow
            key={item.row}
            importId={detail.id}
            item={item}
            mode={item.outcome === "skipped" ? "skipped" : "unmatched"}
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

/**
 * What re-pointing a row did to the entry of the title it had, `old`: the
 * row's plays on it are voided either way, and the entry goes back to the
 * trash only when the row had added it and nothing else was on it.
 */
function previousEntryNote(
  previous: ResolvedImportItem["previous_entry"],
  old: string,
): string | null {
  switch (previous) {
    case "trashed":
      return `${old} went back to the trash`;
    case "kept":
      return `${old} stays in your library without this row's plays`;
    default:
      return null;
  }
}

/**
 * How a row is offered: `unmatched` and `skipped` rows have no title yet
 * (only an unmatched one can be skipped); a `review` row was matched with
 * little certainty, and choosing another title re-points it.
 */
export type ResolveMode = "unmatched" | "skipped" | "review";

export function ResolveRow({
  importId,
  item,
  mode,
}: {
  importId: string;
  item: ImportItem;
  mode: ResolveMode;
}) {
  const queryClient = useQueryClient();
  const { title_language } = usePreferences();
  // Search opens on request: opening it for every row without candidates
  // would send one catalog search per row at once.
  const [searching, setSearching] = useState(false);
  const label = item.source?.title ?? `Row ${item.row}`;
  const current = mode === "review" ? item.title : null;
  // A row under review lists the other titles it might be.
  const candidates = item.candidates.filter(
    (candidate) => candidate.title !== current?.ref,
  );
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
        toast(
          resolved.title
            ? {
                tone: "success",
                title: mode === "review" ? "Re-pointed" : "Matched",
                description: [
                  displayTitle(resolved.title, title_language),
                  current &&
                    previousEntryNote(
                      resolved.previous_entry,
                      displayTitle(current, title_language),
                    ),
                ]
                  .filter(Boolean)
                  .join(". "),
              }
            : { tone: "success", title: "Skipped", description: label },
        );
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
    resolve.mutate({
      id: importId,
      row: item.row,
      data: { action: "match", title },
    });
  }
  const pending = resolve.isPending ? resolve.variables?.data : undefined;
  return (
    <li
      aria-label={label}
      className="flex flex-col gap-3 rounded-lg border border-line p-3"
    >
      <div className="flex flex-col gap-1">
        <span className="font-medium text-fg">
          {label}
          {mode === "skipped" && (
            <span className="ml-2 text-xs font-normal text-fg-muted">
              Skipped
            </span>
          )}
        </span>
        <RawValues item={item} />
        {current && (
          <span className="text-sm text-fg">
            Matched to{" "}
            <Link
              to="/titles/$ref"
              params={{ ref: current.ref }}
              className="text-accent hover:underline"
            >
              {displayTitle(current, title_language)}
            </Link>
            {item.confidence != null && (
              <span className="text-xs text-fg-muted">
                {" "}
                · {percent(item.confidence)} sure
              </span>
            )}
          </span>
        )}
        {item.error && (
          <span className="text-xs text-fg-muted">{item.error}</span>
        )}
      </div>
      {candidates.length > 0 && (
        <ul
          aria-label={`Candidates for ${label}`}
          className="flex flex-col gap-2"
        >
          {candidates.map((candidate) => (
            <li
              key={candidate.title}
              className="flex flex-wrap items-center gap-3 rounded-md bg-surface-sunken px-3 py-2 text-sm"
            >
              <Poster
                src={candidate.poster_url}
                title={candidate.name}
                size="xs"
              />
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
                  pending?.action === "match" &&
                  pending.title === candidate.title
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
            {mode === "review" ? "Search for another title" : "Search"}
          </Button>
        )}
        {mode === "unmatched" && (
          <Button
            variant="ghost"
            size="sm"
            aria-label={`Skip ${label}`}
            loading={pending?.action === "skip"}
            onClick={() =>
              resolve.mutate({
                id: importId,
                row: item.row,
                data: { action: "skip" },
              })
            }
          >
            Skip
          </Button>
        )}
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
