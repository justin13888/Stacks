import { createFileRoute } from "@tanstack/react-router";
import { ListPage } from "@/features/lists/list-page";

export const Route = createFileRoute("/_authenticated/lists/$listId")({
  component: ListRoute,
});

function ListRoute() {
  const { listId } = Route.useParams();
  return <ListPage key={listId} listId={listId} />;
}
