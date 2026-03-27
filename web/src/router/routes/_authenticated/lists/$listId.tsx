import { createLazyFileRoute } from "@tanstack/react-router";
import { DndContext, closestCenter, type DragEndEvent } from "@dnd-kit/core";
import {
  SortableContext,
  verticalListSortingStrategy,
  arrayMove,
} from "@dnd-kit/sortable";
import { useState } from "react";
import ListItemRow from "@/components/lists/list-item-row";

export const Route = createLazyFileRoute("/_authenticated/lists/$listId")({
  component: ListDetailPage,
});

interface ListItem {
  cameoMediaId: string;
  title: string;
  posterUrl?: string;
  position: number;
}

// Placeholder items — replace with API data
const PLACEHOLDER_ITEMS: ListItem[] = [
  { cameoMediaId: "1", title: "Item A", position: 0 },
  { cameoMediaId: "2", title: "Item B", position: 1 },
];

function ListDetailPage() {
  const { listId: _listId } = Route.useParams();
  const [items, setItems] = useState<ListItem[]>(PLACEHOLDER_ITEMS);

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (over && active.id !== over.id) {
      setItems((prev) => {
        const oldIndex = prev.findIndex((i) => i.cameoMediaId === active.id);
        const newIndex = prev.findIndex((i) => i.cameoMediaId === over.id);
        return arrayMove(prev, oldIndex, newIndex).map((item, idx) => ({
          ...item,
          position: idx,
        }));
      });
    }
  }

  return (
    <div data-testid="list-detail-page" className="list-detail-page">
      {/* Single list with drag-and-drop reorder — listId: {listId} */}
      <DndContext collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext
          items={items.map((i) => i.cameoMediaId)}
          strategy={verticalListSortingStrategy}
        >
          {items.map((item) => (
            <ListItemRow key={item.cameoMediaId} item={item} />
          ))}
        </SortableContext>
      </DndContext>
    </div>
  );
}
