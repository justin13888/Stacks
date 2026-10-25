import { infiniteQueryOptions, useInfiniteQuery } from "@tanstack/react-query";
import { History } from "lucide-react";
import { getListImportsQueryKey, listImports } from "@/api/imports/imports";
import type { PageImport } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { LoadMore } from "@/components/ui/load-more";
import { Skeleton } from "@/components/ui/skeleton";
import { Meta, SettingsSection } from "@/features/settings/section";
import { SettingsNav } from "@/features/settings/settings-nav";
import { formatInstant } from "@/lib/format";
import { ImportProgress } from "./import-progress";
import { ImportWizard } from "./import-wizard";
import { isActive, SOURCE_LABELS, STATUS_LABELS } from "./sources";

const HISTORY_POLL_MS = 5000;

export interface ImportSearch {
  /** The import being watched or resolved. */
  import?: string;
}

export function validateImportSearch(
  search: Record<string, unknown>,
): ImportSearch {
  return typeof search.import === "string" && search.import
    ? { import: search.import }
    : {};
}

function importPagesOptions() {
  return infiniteQueryOptions({
    queryKey: [...getListImportsQueryKey(), "pages"] as const,
    queryFn: ({ pageParam, signal }) =>
      listImports({ cursor: pageParam, limit: 20 }, { signal }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last: PageImport) => last.next_cursor ?? undefined,
    // While an import runs, its counts here follow it.
    refetchInterval: (query) =>
      query.state.data?.pages.some((page) =>
        page.items.some((item) => isActive(item.status)),
      )
        ? HISTORY_POLL_MS
        : false,
  });
}

/** `/settings/import`: the wizard, or one import's progress, and the history. */
export function ImportPage({
  importId,
  onOpenImport,
}: {
  importId: string | undefined;
  onOpenImport: (id: string | undefined) => void;
}) {
  return (
    <>
      <PageHeader
        title="Import"
        description="Bring your history from MyAnimeList or TMDB, or restore a Stacks backup."
      />
      <SettingsNav />
      <div className="flex flex-col gap-6">
        <SettingsSection title={importId ? "Import" : "New import"}>
          {importId ? (
            <ImportProgress
              key={importId}
              importId={importId}
              onClose={() => onOpenImport(undefined)}
            />
          ) : (
            <ImportWizard onStarted={onOpenImport} />
          )}
        </SettingsSection>
        <ImportHistory current={importId} onOpen={onOpenImport} />
      </div>
    </>
  );
}

function ImportHistory({
  current,
  onOpen,
}: {
  current: string | undefined;
  onOpen: (id: string) => void;
}) {
  const pages = useInfiniteQuery(importPagesOptions());
  let body: React.ReactNode;
  if (pages.isPending) body = <Skeleton className="h-20" />;
  else if (pages.isError)
    body = (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  else {
    const items = pages.data.pages.flatMap((page) => page.items);
    body =
      items.length === 0 ? (
        <EmptyState
          className="py-6"
          icon={<History />}
          title="No imports yet"
          description="Imports you run show here, so you can come back to resolve their rows."
        />
      ) : (
        <>
          <ul className="flex flex-col divide-y divide-line">
            {items.map((item) => (
              <li
                key={item.id}
                className="flex flex-wrap items-center gap-3 py-3"
              >
                <div className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate font-medium text-fg">
                    {SOURCE_LABELS[item.source]}
                    {item.filename ? `: ${item.filename}` : ""}
                  </span>
                  <Meta>
                    {STATUS_LABELS[item.status]} ·{" "}
                    {formatInstant(item.created_at)} · {item.counts.applied}{" "}
                    matched, {item.counts.unmatched} unmatched,{" "}
                    {item.counts.failed} failed, {item.counts.skipped} skipped
                  </Meta>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={item.id === current}
                  aria-label={`Open import from ${formatInstant(item.created_at)}`}
                  onClick={() => onOpen(item.id)}
                >
                  {item.counts.unmatched > 0 ? "Resolve" : "Open"}
                </Button>
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
    <SettingsSection
      title="Import history"
      description="Every import you ran, newest first."
    >
      {body}
    </SettingsSection>
  );
}
