import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useState } from "react";
import type { ApiListsDtoListResponse } from "@/api/schemas";
import ListCard from "@/components/lists/list-card";
import { authFetch } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/lists/")({
  component: ListsPage,
});

interface CreateListPayload {
  name: string;
  description?: string;
  is_smart?: boolean;
}

function ListsPage() {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [isSmart, setIsSmart] = useState(false);

  const listsKey = ["lists"];

  const lists = useQuery({
    queryKey: listsKey,
    queryFn: ({ signal }) =>
      authFetch<ApiListsDtoListResponse[]>({
        url: "/api/v1/users/me/lists",
        method: "GET",
        signal,
      }),
  });

  const createList = useMutation({
    mutationFn: (payload: CreateListPayload) =>
      authFetch<ApiListsDtoListResponse>({
        url: "/api/v1/users/me/lists",
        method: "POST",
        data: payload,
      }),
    onSuccess: () => {
      setName("");
      setIsSmart(false);
      queryClient.invalidateQueries({ queryKey: listsKey });
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return;
    createList.mutate({ name: trimmed, is_smart: isSmart });
  };

  return (
    <div data-testid="lists-page" className="lists-page">
      <header className="lists-header">
        <h1 className="lists-header-title">Your Lists</h1>
        <form className="lists-create-form" onSubmit={handleSubmit}>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="New list name"
            aria-label="New list name"
            className="lists-create-input"
          />
          <label className="lists-create-smart">
            <input
              type="checkbox"
              checked={isSmart}
              onChange={(e) => setIsSmart(e.target.checked)}
            />
            Smart list
          </label>
          <button
            type="submit"
            disabled={createList.isPending || !name.trim()}
            className="lists-create-button"
          >
            {createList.isPending ? "Creating…" : "Create"}
          </button>
        </form>
      </header>

      {lists.isLoading ? (
        <p className="lists-loading">Loading lists…</p>
      ) : lists.data && lists.data.length > 0 ? (
        <div className="lists-grid">
          {lists.data.map((list) => (
            <Link
              key={list.id}
              to="/lists/$listId"
              params={{ listId: list.id }}
              className="lists-grid-link"
            >
              <ListCard
                list={{
                  id: list.id,
                  name: list.name,
                  description: list.description ?? undefined,
                  itemCount: list.item_count,
                  isSmart: list.is_smart,
                }}
              />
            </Link>
          ))}
        </div>
      ) : (
        <p className="lists-empty">
          You don't have any lists yet. Create one above.
        </p>
      )}
    </div>
  );
}
