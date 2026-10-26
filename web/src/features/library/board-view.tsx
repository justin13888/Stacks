import {
  DndContext,
  type DragEndEvent,
  DragOverlay,
  type DragStartEvent,
  type KeyboardCoordinateGetter,
  KeyboardSensor,
  PointerSensor,
  rectIntersection,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowRightLeft, GripVertical } from "lucide-react";
import { useState } from "react";
import type { LibraryEntry, ListLibraryParams, Status } from "@/api/schemas";
import { IconButton } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ErrorState } from "@/components/ui/error-state";
import { LoadMore } from "@/components/ui/load-more";
import { Poster } from "@/components/ui/poster";
import { Skeleton } from "@/components/ui/skeleton";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import { STATUSES } from "@/features/titles/library-controls";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { cn } from "@/lib/cn";
import { EntryProgress, QuickLogButton, RemoveEntryItem } from "./entry-card";
import { type LibrarySearch, libraryParams } from "./search";
import { libraryPagesOptions, useMoveEntry } from "./use-library";

interface DragData {
  entry: LibraryEntry;
}

/**
 * Arrow Left / Right while a card is picked up (Space or Enter) jump it to
 * the neighbouring column, rather than nudging it a few pixels.
 */
export const columnCoordinates: KeyboardCoordinateGetter = (
  event,
  { context, currentCoordinates },
) => {
  const step =
    event.code === "ArrowRight" ? 1 : event.code === "ArrowLeft" ? -1 : 0;
  if (step === 0) return undefined;
  event.preventDefault();
  const from =
    (context.over?.id as Status | undefined) ??
    (context.active?.data.current as DragData | undefined)?.entry.status;
  const to = from ? STATUSES[STATUSES.indexOf(from) + step] : undefined;
  const rect = to ? context.droppableRects.get(to) : undefined;
  if (!rect) return currentCoordinates;
  return { x: rect.left + 8, y: currentCoordinates.y };
};

/**
 * The status board: a column per status, each its own listing. Dragging a
 * card to another column changes the entry's status; so does the card's
 * "Move to" menu, and picking a card up with Space and moving it with the
 * arrow keys.
 */
export function BoardView({ search }: { search: LibrarySearch }) {
  const move = useMoveEntry();
  const [dragging, setDragging] = useState<LibraryEntry | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: columnCoordinates }),
  );

  const paramsFor = (status: Status): ListLibraryParams =>
    libraryParams(search, { status });

  function moveTo(entry: LibraryEntry, to: Status) {
    if (entry.status === to) return;
    move.mutate({
      entry,
      to,
      fromParams: paramsFor(entry.status),
      toParams: paramsFor(to),
    });
  }

  function onDragStart(event: DragStartEvent) {
    setDragging((event.active.data.current as DragData).entry);
  }

  function onDragEnd(event: DragEndEvent) {
    setDragging(null);
    const entry = (event.active.data.current as DragData | undefined)?.entry;
    const to = event.over?.id as Status | undefined;
    if (entry && to) moveTo(entry, to);
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={rectIntersection}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      onDragCancel={() => setDragging(null)}
      accessibility={{
        screenReaderInstructions: {
          draggable:
            "Press Space or Enter to pick up the card, the Left and Right arrow keys to move it to another status, Space or Enter again to drop it, and Escape to cancel.",
        },
        announcements: {
          onDragStart: () => "Picked up the card.",
          onDragOver: ({ over }) =>
            over
              ? `Over ${STATUS_LABELS[over.id as Status]}.`
              : "Not over a status.",
          onDragEnd: ({ over }) =>
            over
              ? `Moved to ${STATUS_LABELS[over.id as Status]}.`
              : "Dropped where it was.",
          onDragCancel: () => "Cancelled; the card stays where it was.",
        },
      }}
    >
      <div className="-mx-4 flex snap-x gap-3 overflow-x-auto px-4 pb-4">
        {STATUSES.map((status) => (
          <BoardColumn
            key={status}
            status={status}
            params={paramsFor(status)}
            onMove={moveTo}
          />
        ))}
      </div>
      <DragOverlay dropAnimation={null}>
        {dragging && <CardBody entry={dragging} />}
      </DragOverlay>
    </DndContext>
  );
}

function BoardColumn({
  status,
  params,
  onMove,
}: {
  status: Status;
  params: ListLibraryParams;
  onMove: (entry: LibraryEntry, to: Status) => void;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: status });
  const pages = useInfiniteQuery(libraryPagesOptions(params));
  const entries = pages.data?.pages.flatMap((p) => p.items) ?? [];
  const headingId = `board-${status}`;
  return (
    <section
      ref={setNodeRef}
      aria-labelledby={headingId}
      data-board-column={status}
      className={cn(
        "flex w-72 shrink-0 snap-start flex-col gap-2 rounded-xl border border-line bg-surface-sunken p-2 transition-colors",
        isOver && "border-accent bg-accent-soft",
      )}
    >
      <h2
        id={headingId}
        className="flex items-center justify-between px-1 py-1 text-sm font-semibold text-fg"
      >
        {STATUS_LABELS[status]}
        {pages.data && (
          <span className="text-xs font-normal text-fg-muted">
            {entries.length}
            {pages.hasNextPage ? "+" : ""}
          </span>
        )}
      </h2>
      {pages.isPending ? (
        <Skeleton className="h-20" />
      ) : pages.isError ? (
        <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
      ) : (
        <ul className="flex min-h-16 flex-col gap-2">
          {entries.map((entry) => (
            <BoardCard key={entry.title.ref} entry={entry} onMove={onMove} />
          ))}
        </ul>
      )}
      <LoadMore
        hasNextPage={pages.hasNextPage}
        isFetchingNextPage={pages.isFetchingNextPage}
        fetchNextPage={() => pages.fetchNextPage()}
      />
    </section>
  );
}

function BoardCard({
  entry,
  onMove,
}: {
  entry: LibraryEntry;
  onMove: (entry: LibraryEntry, to: Status) => void;
}) {
  const { title_language } = usePreferences();
  const name = displayTitle(entry.title, title_language);
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: entry.title.ref,
    data: { entry } satisfies DragData,
  });
  return (
    <li
      ref={setNodeRef}
      data-board-card={entry.title.ref}
      className={cn(isDragging && "opacity-40")}
    >
      <CardBody
        entry={entry}
        handle={
          <button
            type="button"
            aria-label={`Drag ${name}`}
            className="flex h-10 w-6 cursor-grab items-center justify-center rounded-sm text-fg-subtle hover:text-fg focus-visible:outline-2 focus-visible:outline-ring"
            {...attributes}
            {...listeners}
          >
            <GripVertical aria-hidden className="size-4" />
          </button>
        }
        menu={
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <IconButton
                label={`Move ${name} to…`}
                size="sm"
                icon={<ArrowRightLeft aria-hidden className="size-4" />}
              />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>Move to</DropdownMenuLabel>
              {STATUSES.filter((s) => s !== entry.status).map((status) => (
                <DropdownMenuItem
                  key={status}
                  onSelect={() => onMove(entry, status)}
                >
                  {STATUS_LABELS[status]}
                </DropdownMenuItem>
              ))}
              <DropdownMenuSeparator />
              <RemoveEntryItem entry={entry} name={name} />
            </DropdownMenuContent>
          </DropdownMenu>
        }
      />
    </li>
  );
}

function CardBody({
  entry,
  handle,
  menu,
}: {
  entry: LibraryEntry;
  handle?: React.ReactNode;
  menu?: React.ReactNode;
}) {
  const { title_language } = usePreferences();
  const name = displayTitle(entry.title, title_language);
  return (
    <div className="flex items-center gap-2 rounded-lg border border-line bg-surface p-2 shadow-raised">
      {handle}
      <Poster src={entry.title.poster_url} title={name} size="xs" />
      <Link
        to="/titles/$ref"
        params={{ ref: entry.title.ref }}
        className="min-w-0 flex-1 rounded-sm text-sm font-medium text-fg focus-visible:outline-2 focus-visible:outline-ring"
      >
        <span className="line-clamp-2">{name}</span>
      </Link>
      <EntryProgress entry={entry} size={28} />
      <QuickLogButton entry={entry} />
      {menu}
    </div>
  );
}
