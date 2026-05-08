import { createFileRoute } from "@tanstack/react-router";
import { useApiListsHandlersListLists } from "@/api/lists/lists";
import ListCard from "@/components/lists/list-card";

export const Route = createFileRoute("/_authenticated/lists/")({
  component: ListsPage,
});

function ListsPage() {
  const lists = useApiListsHandlersListLists();

  if (lists.isLoading) {
    return <div data-testid="lists-page">Loading…</div>;
  }

  const items = (lists.data ?? []).map((l) => ({
    id: l.id,
    name: l.name,
    description: l.description ?? undefined,
    itemCount: l.item_count,
    isSmart: l.is_smart,
  }));

  return (
    <div data-testid="lists-page" className="lists-page">
      {items.length === 0 ? (
        <p>No lists yet — create one from a media detail page.</p>
      ) : (
        items.map((list) => <ListCard key={list.id} list={list} />)
      )}
    </div>
  );
}
