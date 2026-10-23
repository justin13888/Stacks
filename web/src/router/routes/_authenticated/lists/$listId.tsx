import { closestCenter, DndContext, type DragEndEvent } from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import type {
  ApiListsDtoListResponse,
  ApiMediaDtoMediaItem,
} from "@/lib/legacy-api";
import ListItemRow from "@/components/lists/list-item-row";
import SmartListBuilder from "@/components/lists/smart-list-builder";
import { authFetch } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/lists/$listId")({
  component: ListDetailPage,
});

interface DisplayItem {
  cameoMediaId: string;
  title: string;
  posterUrl?: string;
  position: number;
}

interface ReorderPayload {
  items: Array<{ cameo_media_id: string; position: number }>;
}

interface UpdateListPayload {
  name?: string;
  description?: string;
  filter_query?: unknown;
}

function ListDetailPage() {
  const { listId } = Route.useParams();
  const queryClient = useQueryClient();

  const listKey = ["list", listId];
  const itemsKey = ["list-items", listId];

  const list = useQuery({
    queryKey: listKey,
    queryFn: ({ signal }) =>
      authFetch<ApiListsDtoListResponse>({
        url: `/api/v1/users/me/lists/${listId}`,
        method: "GET",
        signal,
      }),
  });

  const itemsQuery = useQuery({
    queryKey: itemsKey,
    queryFn: ({ signal }) =>
      authFetch<ApiMediaDtoMediaItem[]>({
        url: `/api/v1/users/me/lists/${listId}/items`,
        method: "GET",
        signal,
      }),
  });

  const [items, setItems] = useState<DisplayItem[]>([]);
  const [filter, setFilter] = useState<object | null>(null);

  useEffect(() => {
    if (itemsQuery.data) {
      setItems(
        itemsQuery.data.map((item, idx) => ({
          cameoMediaId: item.cameo_media_id,
          title: item.title,
          posterUrl: item.poster_url ?? undefined,
          position: idx,
        })),
      );
    }
  }, [itemsQuery.data]);

  const reorderMutation = useMutation({
    mutationFn: (payload: ReorderPayload) =>
      authFetch<void>({
        url: `/api/v1/users/me/lists/${listId}/items/reorder`,
        method: "PUT",
        data: payload,
      }),
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: itemsKey });
    },
  });

  const updateListMutation = useMutation({
    mutationFn: (payload: UpdateListPayload) =>
      authFetch<ApiListsDtoListResponse>({
        url: `/api/v1/users/me/lists/${listId}`,
        method: "PUT",
        data: payload,
      }),
    onSuccess: (next) => {
      queryClient.setQueryData(listKey, next);
      queryClient.invalidateQueries({ queryKey: itemsKey });
    },
  });

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    setItems((prev) => {
      const oldIndex = prev.findIndex((i) => i.cameoMediaId === active.id);
      const newIndex = prev.findIndex((i) => i.cameoMediaId === over.id);
      if (oldIndex === -1 || newIndex === -1) return prev;
      const next = arrayMove(prev, oldIndex, newIndex).map((item, idx) => ({
        ...item,
        position: idx,
      }));
      reorderMutation.mutate({
        items: next.map((i) => ({
          cameo_media_id: i.cameoMediaId,
          position: i.position,
        })),
      });
      return next;
    });
  }

  const handleFilterChange = (next: object) => {
    setFilter(next);
    updateListMutation.mutate({ filter_query: next });
  };

  if (list.isLoading) {
    return (
      <div data-testid="list-detail-page" className="list-detail-page">
        <p>Loading list…</p>
      </div>
    );
  }

  if (list.isError || !list.data) {
    return (
      <div data-testid="list-detail-page" className="list-detail-page">
        <p>Couldn't load this list.</p>
      </div>
    );
  }

  return (
    <div data-testid="list-detail-page" className="list-detail-page">
      <header className="list-detail-header">
        <h1 className="list-detail-name">{list.data.name}</h1>
        {list.data.description && (
          <p className="list-detail-description">{list.data.description}</p>
        )}
        <span className="list-detail-count">
          {list.data.item_count} item{list.data.item_count === 1 ? "" : "s"}
        </span>
      </header>

      {list.data.is_smart && (
        <section className="list-detail-smart-builder">
          <h2 className="list-detail-section-title">Smart Filter</h2>
          <SmartListBuilder value={filter} onChange={handleFilterChange} />
        </section>
      )}

      {itemsQuery.isLoading ? (
        <p>Loading items…</p>
      ) : items.length === 0 ? (
        <p className="list-detail-empty">
          {list.data.is_smart
            ? "No items match the current filter."
            : "This list is empty. Add items from a media detail page."}
        </p>
      ) : list.data.is_smart ? (
        // Smart lists are computed — drag handles are meaningless
        <div className="list-detail-items">
          {items.map((item) => (
            <ListItemRow key={item.cameoMediaId} item={item} />
          ))}
        </div>
      ) : (
        <DndContext
          collisionDetection={closestCenter}
          onDragEnd={handleDragEnd}
        >
          <SortableContext
            items={items.map((i) => i.cameoMediaId)}
            strategy={verticalListSortingStrategy}
          >
            <div className="list-detail-items">
              {items.map((item) => (
                <ListItemRow key={item.cameoMediaId} item={item} />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      )}
    </div>
  );
}
