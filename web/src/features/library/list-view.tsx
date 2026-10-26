import { Link } from "@tanstack/react-router";
import { ArrowDown } from "lucide-react";
import type { LibraryEntry, Sort } from "@/api/schemas";
import { CATEGORY_NAMES } from "@/components/search/categories";
import { Poster } from "@/components/ui/poster";
import { Rating } from "@/components/ui/rating";
import { StatusBadge } from "@/components/ui/status-badge";
import { TagList } from "@/features/tags/tag-chip";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { cn } from "@/lib/cn";
import { formatInstant } from "@/lib/format";
import { EntryMenu, EntryProgress, QuickLogButton } from "./entry-card";

function SortHeader({
  label,
  sort,
  active,
  onSort,
  className,
}: {
  label: string;
  sort: Sort;
  active: Sort;
  onSort: (sort: Sort) => void;
  className?: string;
}) {
  const isActive = sort === active;
  return (
    <th
      scope="col"
      aria-sort={
        isActive ? (sort === "title" ? "ascending" : "descending") : undefined
      }
      className={cn("px-3 py-2 font-medium", className)}
    >
      <button
        type="button"
        onClick={() => onSort(sort)}
        className={cn(
          "inline-flex items-center gap-1 rounded-sm hover:text-fg focus-visible:outline-2 focus-visible:outline-ring",
          isActive && "text-fg",
        )}
      >
        {label}
        {isActive && (
          <ArrowDown
            aria-hidden
            className={cn("size-3.5", sort === "title" && "rotate-180")}
          />
        )}
      </button>
    </th>
  );
}

/**
 * The dense view: one row per entry. Each sortable header sets the
 * listing's order (the server sorts: title A–Z, the rest newest or highest
 * first).
 */
export function ListView({
  entries,
  sort,
  onSort,
  selected,
  onSelectedChange,
}: {
  entries: readonly LibraryEntry[];
  sort: Sort;
  onSort: (sort: Sort) => void;
  /** The refs ticked for a bulk action; omit for no selection column. */
  selected?: ReadonlySet<string>;
  onSelectedChange?: (selected: ReadonlySet<string>) => void;
}) {
  const { title_language, score_format } = usePreferences();
  const selectable = selected !== undefined && onSelectedChange !== undefined;
  const allSelected =
    selectable &&
    entries.length > 0 &&
    entries.every((e) => selected.has(e.title.ref));
  function toggle(ref: string) {
    if (!selectable) return;
    const next = new Set(selected);
    if (!next.delete(ref)) next.add(ref);
    onSelectedChange(next);
  }
  return (
    <div className="-mx-4 overflow-x-auto px-4">
      <table className="w-full min-w-[36rem] border-separate border-spacing-0 text-sm">
        <thead className="text-left text-xs text-fg-muted">
          <tr>
            {selectable && (
              <th scope="col" className="w-8 px-3 py-2">
                <input
                  type="checkbox"
                  aria-label="Select every title"
                  checked={allSelected}
                  onChange={() =>
                    onSelectedChange(
                      allSelected
                        ? new Set()
                        : new Set(entries.map((e) => e.title.ref)),
                    )
                  }
                  className="size-4 accent-(--color-accent)"
                />
              </th>
            )}
            <SortHeader
              label="Title"
              sort="title"
              active={sort}
              onSort={onSort}
            />
            <th scope="col" className="px-3 py-2 font-medium">
              Status
            </th>
            <SortHeader
              label="Score"
              sort="score"
              active={sort}
              onSort={onSort}
            />
            <th scope="col" className="px-3 py-2 font-medium">
              Progress
            </th>
            <SortHeader
              label="Updated"
              sort="updated"
              active={sort}
              onSort={onSort}
              className="hidden md:table-cell"
            />
            <SortHeader
              label="Added"
              sort="added"
              active={sort}
              onSort={onSort}
              className="hidden lg:table-cell"
            />
          </tr>
        </thead>
        <tbody>
          {entries.map((entry) => {
            const name = displayTitle(entry.title, title_language);
            return (
              <tr key={entry.title.ref} className="hover:bg-surface-raised">
                {selectable && (
                  <td className="border-t border-line px-3 py-2">
                    <input
                      type="checkbox"
                      aria-label={`Select ${name}`}
                      checked={selected.has(entry.title.ref)}
                      onChange={() => toggle(entry.title.ref)}
                      className="size-4 accent-(--color-accent)"
                    />
                  </td>
                )}
                <td className="border-t border-line px-3 py-2">
                  <Link
                    to="/titles/$ref"
                    params={{ ref: entry.title.ref }}
                    className="flex items-center gap-3 rounded-sm focus-visible:outline-2 focus-visible:outline-ring"
                  >
                    <Poster
                      src={entry.title.poster_url}
                      title={name}
                      size="xs"
                    />
                    <span className="flex min-w-0 flex-col">
                      <span className="truncate font-medium text-fg">
                        {name}
                      </span>
                      <span className="text-xs text-fg-muted">
                        {[
                          entry.title.year,
                          CATEGORY_NAMES[entry.title.category],
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </span>
                    </span>
                  </Link>
                  <TagList
                    tags={entry.tags}
                    label={`Tags on ${name}`}
                    className="mt-1 pl-13"
                  />
                </td>
                <td className="border-t border-line px-3 py-2">
                  <StatusBadge status={entry.status} />
                </td>
                <td className="border-t border-line px-3 py-2">
                  <Rating score={entry.score} format={score_format} />
                </td>
                <td className="border-t border-line px-3 py-2">
                  <span className="flex items-center gap-2">
                    <EntryProgress entry={entry} size={28} />
                    <QuickLogButton entry={entry} />
                    <EntryMenu entry={entry} />
                  </span>
                </td>
                <td className="hidden border-t border-line px-3 py-2 text-fg-muted md:table-cell">
                  {formatInstant(entry.updated_at)}
                </td>
                <td className="hidden border-t border-line px-3 py-2 text-fg-muted lg:table-cell">
                  {formatInstant(entry.added_at)}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
