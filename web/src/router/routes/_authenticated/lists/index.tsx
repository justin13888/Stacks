import { createLazyFileRoute } from "@tanstack/react-router";
import ListCard from "@/components/lists/list-card";

export const Route = createLazyFileRoute("/_authenticated/lists/")({
  component: ListsPage,
});

// Placeholder lists — replace with API data
const PLACEHOLDER_LISTS = [
  {
    id: "1",
    name: "Favourites",
    description: "My all-time favourites",
    itemCount: 12,
    isSmart: false,
  },
  {
    id: "2",
    name: "Watch Next",
    description: undefined,
    itemCount: 7,
    isSmart: true,
  },
];

function ListsPage() {
  return (
    <div data-testid="lists-page" className="lists-page">
      {/* All lists overview */}
      {PLACEHOLDER_LISTS.map((list) => (
        <ListCard key={list.id} list={list} />
      ))}
    </div>
  );
}
