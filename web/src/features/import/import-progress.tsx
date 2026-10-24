import { ArrowLeft } from "lucide-react";
import { useGetImport } from "@/api/imports/imports";
import type { ImportCounts, ImportDetail } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { formatInstant } from "@/features/calendar/dates";
import { IMPORT_STEPS, Steps } from "./import-wizard";
import { MatchResolution, ResolveRow } from "./match-resolution";
import {
  isActive,
  lowConfidenceRows,
  SOURCE_LABELS,
  STATUS_LABELS,
} from "./sources";

/** How often a running import is read again. */
export const POLL_MS = 2000;

const COUNT_LABELS: readonly [keyof ImportCounts, string][] = [
  ["applied", "Matched"],
  ["unmatched", "Unmatched"],
  ["failed", "Failed"],
  ["skipped", "Skipped"],
  ["pending_retry", "Waiting to retry"],
];

/**
 * Steps 2 to 4 of an import: its progress while the worker reads it (the
 * import named by the 202's `Location`, polled until it settles), then the
 * summary and the rows to resolve by hand.
 */
export function ImportProgress({
  importId,
  onClose,
}: {
  importId: string;
  onClose: () => void;
}) {
  const detail = useGetImport(importId, {
    query: {
      // A failed read (a stale `?import=` id answers 404) stops the polling;
      // the error's Retry starts it again.
      refetchInterval: (query) =>
        query.state.status === "error" ||
        (query.state.data && !isActive(query.state.data.status))
          ? false
          : POLL_MS,
    },
  });

  let body: React.ReactNode;
  if (detail.isPending) body = <Skeleton className="h-40" />;
  else if (detail.isError)
    body = (
      <ErrorState error={detail.error} onRetry={() => void detail.refetch()} />
    );
  else body = <ImportBody detail={detail.data} />;

  return (
    <div className="flex flex-col gap-4">
      <Button
        variant="ghost"
        size="sm"
        className="self-start"
        onClick={onClose}
      >
        <ArrowLeft aria-hidden className="size-4" />
        New import
      </Button>
      {body}
    </div>
  );
}

function ImportBody({ detail }: { detail: ImportDetail }) {
  const active = isActive(detail.status);
  const { counts } = detail;
  const done =
    counts.applied + counts.skipped + counts.unmatched + counts.failed;
  return (
    <>
      <Steps labels={IMPORT_STEPS} current={active ? 1 : 2} />
      <div className="flex flex-col gap-1">
        <h3 className="font-semibold text-fg">
          {SOURCE_LABELS[detail.source]}
          {detail.filename ? `: ${detail.filename}` : ""}
        </h3>
        <p className="text-sm text-fg-muted">
          {STATUS_LABELS[detail.status]} · started{" "}
          {formatInstant(detail.started_at ?? detail.created_at)}
          {detail.completed_at
            ? ` · finished ${formatInstant(detail.completed_at)}`
            : ""}
        </p>
      </div>
      {active && (
        <div className="flex flex-col gap-1.5">
          <progress
            aria-label="Import progress"
            max={Math.max(counts.total, 1)}
            value={counts.total ? done : undefined}
            className="h-2 w-full accent-(--color-accent)"
          />
          <p className="text-sm text-fg-muted" aria-live="polite">
            {counts.total
              ? `${done} of ${counts.total} rows read`
              : "Reading the file…"}
          </p>
        </div>
      )}
      {detail.status === "failed" && (
        <p role="alert" className="text-sm text-danger">
          The import failed{detail.error ? `: ${detail.error}` : "."}
        </p>
      )}
      <dl
        aria-label="Import summary"
        className="grid grid-cols-2 gap-3 sm:grid-cols-5"
      >
        {COUNT_LABELS.map(([key, label]) => (
          <div key={key} className="rounded-lg border border-line p-3">
            <dt className="text-xs text-fg-muted">{label}</dt>
            <dd className="text-xl font-semibold tabular-nums text-fg">
              {counts[key]}
            </dd>
          </div>
        ))}
      </dl>
      {!active && (
        <>
          <MatchResolution detail={detail} />
          <LowConfidence detail={detail} />
          <FailedRows detail={detail} />
        </>
      )}
    </>
  );
}

/**
 * Rows matched with little certainty, each with the other titles it might
 * be: choosing one re-points the row, and the entry it added goes to the
 * trash.
 */
function LowConfidence({ detail }: { detail: ImportDetail }) {
  const rows = lowConfidenceRows(detail.items);
  if (rows.length === 0) return null;
  return (
    <section
      aria-label="Low-confidence matches"
      className="flex flex-col gap-3"
    >
      <h3 className="font-semibold text-fg">Worth a check</h3>
      <p className="text-sm text-fg-muted">
        These were matched, but not with certainty. If one is the wrong title,
        pick the right one: the entry the import added moves to the trash.
      </p>
      <ul className="flex flex-col gap-3">
        {rows.map((item) => (
          <ResolveRow
            key={item.row}
            importId={detail.id}
            item={item}
            mode="review"
          />
        ))}
      </ul>
    </section>
  );
}

function FailedRows({ detail }: { detail: ImportDetail }) {
  const rows = detail.items.filter((item) => item.outcome === "failed");
  if (rows.length === 0) return null;
  return (
    <section aria-label="Failed rows" className="flex flex-col gap-2">
      <h3 className="font-semibold text-fg">Couldn't import</h3>
      <ul className="flex flex-col divide-y divide-line text-sm">
        {rows.map((item) => (
          <li key={item.row} className="flex flex-col py-2">
            <span>{item.source?.title ?? `Row ${item.row}`}</span>
            {item.error && (
              <span className="text-xs text-fg-muted">{item.error}</span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
