import {
  keepPreviousData,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { BookmarkPlus, CircleCheck, CornerDownLeft } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useState } from "react";
import {
  getSearchTitlesQueryKey,
  useSearchTitles,
} from "@/api/catalog/catalog";
import {
  getListLibraryQueryKey,
  logPlays,
  putLibraryEntry,
} from "@/api/library/library";
import type { Category, TitleSummary } from "@/api/schemas";
import { Combobox } from "@/components/ui/combobox";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { ErrorState } from "@/components/ui/error-state";
import { Poster } from "@/components/ui/poster";
import { Skeleton } from "@/components/ui/skeleton";
import { StatusBadge } from "@/components/ui/status-badge";
import { toast } from "@/components/ui/toast";
import { useDebouncedValue } from "@/hooks/use-debounced-value";
import { cn } from "@/lib/cn";
import { CATEGORIES, CATEGORY_LABELS, CATEGORY_NAMES } from "./categories";

export const SEARCH_DEBOUNCE_MS = 250;

export interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

type QuickAction = "plan" | "watched";

/** "Add to Planning" only for titles not yet tracked, so nothing is overwritten. */
export function canAddToPlanning(title: TitleSummary): boolean {
  return title.library == null;
}

/** "Mark watched" logs one play of a movie that isn't already completed. */
export function canMarkWatched(title: TitleSummary): boolean {
  return title.kind === "movie" && title.library?.status !== "completed";
}

function subtitle(title: TitleSummary): string {
  return [title.year, CATEGORY_NAMES[title.category]]
    .filter(Boolean)
    .join(" · ");
}

/**
 * The ⌘K / Ctrl+K palette: search every provider, narrow by category, and act
 * on a result without leaving the keyboard. Enter opens a title, Alt+P adds it
 * to Planning, Alt+W marks a movie watched.
 */
export function CommandPalette({ open, onOpenChange }: CommandPaletteProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {open && <PaletteBody onClose={() => onOpenChange(false)} />}
    </Dialog>
  );
}

function PaletteBody({ onClose }: { onClose: () => void }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [text, setText] = useState("");
  const [category, setCategory] = useState<Category | null>(null);
  const q = useDebouncedValue(text.trim(), SEARCH_DEBOUNCE_MS);

  const search = useSearchTitles(
    { q, category },
    { query: { enabled: q.length > 0, placeholderData: keepPreviousData } },
  );
  const items = q.length > 0 ? (search.data?.items ?? []) : [];

  const action = useMutation({
    mutationFn: async ({
      title,
      kind,
    }: {
      title: TitleSummary;
      kind: QuickAction;
    }) => {
      if (kind === "plan") {
        await putLibraryEntry(title.ref, { status: "planning" });
      } else {
        await logPlays(title.ref, { kind: "movie" });
      }
    },
    onSuccess: async (_data, { title, kind }) => {
      toast({
        tone: "success",
        title: kind === "plan" ? "Added to Planning" : "Marked as watched",
        description: title.title,
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: getSearchTitlesQueryKey() }),
        queryClient.invalidateQueries({ queryKey: getListLibraryQueryKey() }),
      ]);
    },
  });

  function openTitle(title: TitleSummary) {
    onClose();
    void navigate({ to: "/titles/$ref", params: { ref: title.ref } });
  }

  function run(title: TitleSummary, kind: QuickAction) {
    if (kind === "plan" && !canAddToPlanning(title)) return;
    if (kind === "watched" && !canMarkWatched(title)) return;
    action.mutate({ title, kind });
  }

  function onItemKeyDown(
    event: KeyboardEvent<HTMLInputElement>,
    title: TitleSummary,
  ) {
    if (!event.altKey) return false;
    // `code` survives keyboard layouts and macOS Option-key characters.
    if (event.code === "KeyP") {
      run(title, "plan");
      return true;
    }
    if (event.code === "KeyW") {
      run(title, "watched");
      return true;
    }
    return false;
  }

  let status: ReactNode;
  if (q.length === 0) {
    status = (
      <p className="px-5 py-8 text-center text-sm text-fg-muted">
        Search movies, shows, anime and dramas.
      </p>
    );
  } else if (search.isPending) {
    status = (
      <div
        role="status"
        aria-label="Searching"
        className="flex flex-col gap-2 p-3"
      >
        {[0, 1, 2].map((i) => (
          <div key={i} className="flex items-center gap-3 px-2">
            <Skeleton className="h-15 w-10" />
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-4 w-1/2" />
              <Skeleton className="h-3 w-1/4" />
            </div>
          </div>
        ))}
      </div>
    );
  } else if (search.isError) {
    status = (
      <ErrorState error={search.error} onRetry={() => void search.refetch()} />
    );
  } else {
    status = (
      <p className="px-5 py-8 text-center text-sm text-fg-muted">
        No titles match “{q}”.
      </p>
    );
  }

  const degraded = (search.data?.degraded.length ?? 0) > 0;

  return (
    <DialogContent
      title="Search titles"
      hideTitle
      hideClose
      className="top-[12vh] max-h-[76vh] w-[min(94vw,40rem)] translate-y-0 gap-0 p-0"
    >
      <Combobox
        label="Search titles"
        hideLabel
        placeholder="Search titles…"
        inputValue={text}
        onInputValueChange={setText}
        items={items}
        getKey={(title) => title.ref}
        getLabel={(title) =>
          [title.title, subtitle(title)].filter(Boolean).join(", ")
        }
        onSelect={openTitle}
        onItemKeyDown={onItemKeyDown}
        status={status}
        className="min-h-0 flex-1"
        renderItem={(title, { active }) => (
          <ResultRow
            title={title}
            active={active}
            busy={action.isPending && action.variables?.title.ref === title.ref}
            onAction={(kind) => run(title, kind)}
          />
        )}
      />

      <fieldset className="flex flex-wrap gap-1.5 border-line border-t px-4 py-3">
        <legend className="sr-only">Category</legend>
        <CategoryChip
          label="All"
          pressed={category === null}
          onClick={() => setCategory(null)}
        />
        {CATEGORIES.map((c) => (
          <CategoryChip
            key={c}
            label={CATEGORY_LABELS[c]}
            pressed={category === c}
            onClick={() => setCategory(category === c ? null : c)}
          />
        ))}
      </fieldset>

      {degraded && (
        <p
          role="status"
          className="border-line border-t px-4 py-2 text-xs text-fg-muted"
        >
          A provider didn't answer, so results may be incomplete.
        </p>
      )}

      <footer className="flex flex-wrap items-center gap-x-4 gap-y-1 border-line border-t bg-surface-raised px-4 py-2 text-2xs text-fg-subtle">
        <Hint keys="↑ ↓">move</Hint>
        <Hint keys="Enter">open</Hint>
        <Hint keys="Alt P">add to Planning</Hint>
        <Hint keys="Alt W">mark watched</Hint>
        <Hint keys="Esc">close</Hint>
      </footer>
    </DialogContent>
  );
}

function ResultRow({
  title,
  active,
  busy,
  onAction,
}: {
  title: TitleSummary;
  active: boolean;
  busy: boolean;
  onAction: (kind: QuickAction) => void;
}) {
  return (
    <div
      className={cn(
        "mx-2 flex items-center gap-3 rounded-md px-3 py-2 transition-colors duration-(--duration-instant)",
        active && "bg-accent-soft",
      )}
    >
      <Poster src={title.poster_url} title={title.title} size="xs" />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm font-medium text-fg">
          {title.title}
        </span>
        <span className="truncate text-xs text-fg-muted">
          {subtitle(title)}
        </span>
      </div>
      {title.library && (
        <StatusBadge
          status={title.library.status}
          rewatching={title.library.rewatching}
        />
      )}
      <div className={cn("flex items-center gap-1", !active && "opacity-0")}>
        {canAddToPlanning(title) && (
          <RowAction
            label="Add to Planning"
            disabled={busy}
            onClick={() => onAction("plan")}
          >
            <BookmarkPlus aria-hidden className="size-4" />
          </RowAction>
        )}
        {canMarkWatched(title) && (
          <RowAction
            label="Mark watched"
            disabled={busy}
            onClick={() => onAction("watched")}
          >
            <CircleCheck aria-hidden className="size-4" />
          </RowAction>
        )}
        {active && (
          <CornerDownLeft aria-hidden className="size-4 text-fg-subtle" />
        )}
      </div>
    </div>
  );
}

/**
 * A pointer shortcut inside an option. It is not a tab stop (the option list
 * is driven from the text field); the keyboard uses Alt+P / Alt+W instead.
 */
function RowAction({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      tabIndex={-1}
      title={label}
      aria-label={label}
      disabled={disabled}
      onMouseDown={(event) => event.preventDefault()}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      className="inline-flex size-8 items-center justify-center rounded-sm text-fg-muted hover:bg-surface hover:text-accent disabled:opacity-50"
    >
      {children}
    </button>
  );
}

function CategoryChip({
  label,
  pressed,
  onClick,
}: {
  label: string;
  pressed: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={cn(
        "rounded-full border px-3 py-1 text-xs font-medium transition-colors duration-(--duration-fast)",
        "focus-visible:outline-2 focus-visible:outline-ring",
        pressed
          ? "border-accent bg-accent text-fg-on-accent"
          : "border-line text-fg-muted hover:border-line-strong hover:text-fg",
      )}
    >
      {label}
    </button>
  );
}

function Hint({ keys, children }: { keys: string; children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1">
      <kbd className="rounded-xs border border-line bg-surface px-1 font-mono">
        {keys}
      </kbd>
      {children}
    </span>
  );
}
