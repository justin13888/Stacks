import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import {
  ArrowDown,
  ArrowUp,
  GripVertical,
  ListVideo,
  Pencil,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { useState } from "react";
import {
  restoreList,
  useDeleteList,
  useGetList,
  useRemoveListItem,
  useUpdateList,
} from "@/api/lists/lists";
import type { List, ListItem } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { CATEGORY_NAMES } from "@/components/search/categories";
import { Button, IconButton } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Poster } from "@/components/ui/poster";
import { Skeleton } from "@/components/ui/skeleton";
import { StatusBadge } from "@/components/ui/status-badge";
import { toast } from "@/components/ui/toast";
import { LoadMore } from "@/features/library/load-more";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { reportFailure } from "@/features/titles/use-tracking";
import { cn } from "@/lib/cn";
import { ListFormDialog } from "./list-form";
import {
  invalidateLists,
  listItemsPagesOptions,
  placementFor,
  useReorderItem,
} from "./use-lists";

/** One list: its titles in order, reorderable by drag or by keyboard. */
export function ListPage({ listId }: { listId: string }) {
  const list = useGetList(listId);
  if (list.isPending) {
    return (
      <div
        role="status"
        aria-label="Loading list"
        className="flex flex-col gap-3"
      >
        <Skeleton className="h-9 w-1/3" />
        <Skeleton className="h-16" />
        <Skeleton className="h-16" />
      </div>
    );
  }
  if (list.isError) {
    return (
      <ErrorState error={list.error} onRetry={() => void list.refetch()} />
    );
  }
  return <ListBody list={list.data} />;
}

function ListBody({ list }: { list: List }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [editing, setEditing] = useState(false);
  const update = useUpdateList({
    mutation: {
      onSuccess: () => setEditing(false),
      onError: reportFailure,
      onSettled: () => invalidateLists(queryClient, list.id),
    },
  });
  const remove = useDeleteList({
    mutation: {
      onSuccess: () => {
        toast({
          title: `Moved ${list.name} to the trash`,
          action: {
            label: "Undo",
            onClick: () => {
              void restoreList(list.id)
                .then(() =>
                  navigate({
                    to: "/lists/$listId",
                    params: { listId: list.id },
                  }),
                )
                .catch(reportFailure)
                .finally(() => invalidateLists(queryClient, list.id));
            },
          },
        });
        void navigate({ to: "/lists" });
      },
      onError: reportFailure,
      onSettled: () => invalidateLists(queryClient),
    },
  });

  return (
    <>
      <nav aria-label="Breadcrumb" className="mb-2 text-sm">
        <Link to="/lists" className="text-fg-muted hover:text-fg">
          Lists
        </Link>
      </nav>
      <PageHeader
        title={list.name}
        description={list.description ?? undefined}
        actions={
          <div className="flex gap-2">
            <Button variant="secondary" onClick={() => setEditing(true)}>
              <Pencil aria-hidden className="size-4" />
              Edit
            </Button>
            <Button
              variant="ghost"
              loading={remove.isPending}
              onClick={() => remove.mutate({ id: list.id })}
            >
              <Trash2 aria-hidden className="size-4" />
              Delete
            </Button>
          </div>
        }
      />
      {list.smart && (
        <p className="mb-4 flex items-center gap-1.5 text-sm text-fg-muted">
          <Sparkles aria-hidden className="size-4 text-accent" />A smart list:
          its filter picks the titles, in its feed's order.
        </p>
      )}
      <Items list={list} />
      <ListFormDialog
        open={editing}
        onOpenChange={setEditing}
        title="Edit list"
        submitLabel="Save"
        initial={{ name: list.name, description: list.description ?? null }}
        pending={update.isPending}
        onSubmit={(fields) => update.mutate({ id: list.id, data: fields })}
      />
    </>
  );
}

function Items({ list }: { list: List }) {
  const queryClient = useQueryClient();
  const { title_language } = usePreferences();
  const pages = useInfiniteQuery(listItemsPagesOptions(list.id));
  const reorder = useReorderItem(list.id);
  const removeItem = useRemoveListItem({
    mutation: {
      onError: reportFailure,
      onSettled: () => invalidateLists(queryClient, list.id),
    },
  });
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  if (pages.isPending) return <Skeleton className="h-48" />;
  // A failed refetch keeps the titles on screen (the query toasts it).
  if (!pages.data) {
    return (
      <ErrorState error={pages.error} onRetry={() => void pages.refetch()} />
    );
  }
  const items = pages.data.pages.flatMap((page) => page.items);
  if (items.length === 0) {
    return (
      <EmptyState
        icon={<ListVideo />}
        title="This list is empty"
        description={
          list.smart
            ? "Nothing passes its filter right now."
            : "Add titles from any card's menu in Discover, or from a title's page."
        }
      />
    );
  }

  const sortable = !list.smart;
  const hasMore = pages.hasNextPage;
  function move(from: number, to: number) {
    const item = items[from];
    if (!item || to < 0 || to >= items.length || from === to) return;
    reorder.mutate({
      ref: item.title.ref,
      from,
      to,
      placement: placementFor(items, from, to, hasMore),
    });
  }

  function onDragEnd(event: DragEndEvent) {
    if (!event.over) return;
    const refs = items.map((i) => i.title.ref);
    move(
      refs.indexOf(String(event.active.id)),
      refs.indexOf(String(event.over.id)),
    );
  }

  const nameOf = (item: ListItem) => displayTitle(item.title, title_language);
  const rows = (
    <ol className="flex flex-col gap-2">
      {items.map((item, index) => (
        <Row
          key={item.title.ref}
          item={item}
          name={nameOf(item)}
          position={index + 1}
          sortable={sortable}
          onMoveUp={index > 0 ? () => move(index, index - 1) : undefined}
          onMoveDown={
            index < items.length - 1 ? () => move(index, index + 1) : undefined
          }
          onRemove={
            sortable
              ? () =>
                  removeItem.mutate(
                    { id: list.id, ref: item.title.ref },
                    {
                      onSuccess: () =>
                        toast({
                          title: "Removed from the list",
                          description: nameOf(item),
                        }),
                    },
                  )
              : undefined
          }
        />
      ))}
    </ol>
  );

  return (
    <>
      {sortable ? (
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragEnd={onDragEnd}
          accessibility={{
            screenReaderInstructions: {
              draggable:
                "Press Space or Enter to pick up the title, the Up and Down arrow keys to move it, Space or Enter again to drop it, and Escape to cancel.",
            },
            announcements: {
              onDragStart: () => "Picked up the title.",
              onDragOver: ({ over }) =>
                over
                  ? `Over position ${items.findIndex((i) => i.title.ref === over.id) + 1}.`
                  : "Not over the list.",
              onDragEnd: ({ over }) =>
                over
                  ? `Dropped at position ${items.findIndex((i) => i.title.ref === over.id) + 1}.`
                  : "Dropped where it was.",
              onDragCancel: () => "Cancelled; the title stays where it was.",
            },
          }}
        >
          <SortableContext
            items={items.map((i) => i.title.ref)}
            strategy={verticalListSortingStrategy}
          >
            {rows}
          </SortableContext>
        </DndContext>
      ) : (
        rows
      )}
      <LoadMore
        hasNextPage={pages.hasNextPage}
        isFetchingNextPage={pages.isFetchingNextPage}
        fetchNextPage={() => pages.fetchNextPage()}
      />
    </>
  );
}

function Row({
  item,
  name,
  position,
  sortable,
  onMoveUp,
  onMoveDown,
  onRemove,
}: {
  item: ListItem;
  name: string;
  position: number;
  sortable: boolean;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  onRemove?: () => void;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: item.title.ref, disabled: !sortable });
  const { title } = item;
  return (
    <li
      ref={setNodeRef}
      data-list-item={title.ref}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn(
        "flex items-center gap-3 rounded-lg border border-line bg-surface p-2 shadow-raised",
        isDragging && "relative z-10 opacity-80 shadow-overlay",
      )}
    >
      {sortable && (
        <button
          type="button"
          aria-label={`Drag ${name}`}
          className="flex h-10 w-6 cursor-grab touch-none items-center justify-center rounded-sm text-fg-subtle hover:text-fg focus-visible:outline-2 focus-visible:outline-ring"
          {...attributes}
          {...listeners}
        >
          <GripVertical aria-hidden className="size-4" />
        </button>
      )}
      <span className="w-6 text-right text-sm tabular-nums text-fg-subtle">
        {position}
      </span>
      <Poster src={title.poster_url} title={name} size="xs" />
      <div className="flex min-w-0 flex-1 flex-col">
        <Link
          to="/titles/$ref"
          params={{ ref: title.ref }}
          className="truncate font-medium text-fg hover:underline"
        >
          {name}
        </Link>
        <span className="truncate text-xs text-fg-muted">
          {[title.year, CATEGORY_NAMES[title.category]]
            .filter(Boolean)
            .join(" · ")}
          {item.notes ? ` · ${item.notes}` : ""}
        </span>
      </div>
      {title.library && (
        <StatusBadge
          status={title.library.status}
          rewatching={title.library.rewatching}
        />
      )}
      {sortable && (
        <div className="flex gap-0.5">
          <IconButton
            label={`Move ${name} up`}
            size="sm"
            icon={<ArrowUp aria-hidden className="size-4" />}
            disabled={!onMoveUp}
            onClick={onMoveUp}
          />
          <IconButton
            label={`Move ${name} down`}
            size="sm"
            icon={<ArrowDown aria-hidden className="size-4" />}
            disabled={!onMoveDown}
            onClick={onMoveDown}
          />
          {onRemove && (
            <IconButton
              label={`Remove ${name} from the list`}
              size="sm"
              icon={<X aria-hidden className="size-4" />}
              onClick={onRemove}
            />
          )}
        </div>
      )}
    </li>
  );
}
