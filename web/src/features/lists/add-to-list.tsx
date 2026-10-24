import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ListPlus } from "lucide-react";
import { useAddListItem, useListLists } from "@/api/lists/lists";
import type { List, TitleSummary } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { ErrorState } from "@/components/ui/error-state";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { reportFailure } from "@/features/titles/use-tracking";
import { isApiProblem } from "@/lib/api-client";
import { invalidateLists } from "./use-lists";

/** How many lists the picker offers. */
const PICKER_LISTS = 200;

/**
 * Picks one of the caller's manual lists to add `title` to. A smart list is
 * filled by its filter, so it is not offered.
 */
export function AddToListDialog({
  title,
  open,
  onOpenChange,
}: {
  title: TitleSummary;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Add to a list"
        description={`Choose a list for ${title.title}.`}
      >
        {open && (
          <ListChoices title={title} onDone={() => onOpenChange(false)} />
        )}
      </DialogContent>
    </Dialog>
  );
}

function ListChoices({
  title,
  onDone,
}: {
  title: TitleSummary;
  onDone: () => void;
}) {
  const queryClient = useQueryClient();
  const lists = useListLists({ limit: PICKER_LISTS });
  const add = useAddListItem({
    mutation: {
      onSuccess: (_item, { id }) => {
        toast({
          tone: "success",
          title: `Added to ${nameOf(lists.data?.items, id)}`,
          description: title.title,
        });
        onDone();
      },
      onError: (error, { id }) => {
        if (isApiProblem(error) && error.status === 409) {
          toast({
            title: `Already in ${nameOf(lists.data?.items, id)}`,
            description: title.title,
          });
          onDone();
          return;
        }
        reportFailure(error);
      },
      onSettled: (_item, _error, { id }) => invalidateLists(queryClient, id),
    },
  });

  if (lists.isPending) return <Skeleton className="h-24" />;
  if (lists.isError) {
    return (
      <ErrorState error={lists.error} onRetry={() => void lists.refetch()} />
    );
  }
  const manual = lists.data.items.filter((list) => !list.smart);
  if (manual.length === 0) {
    return (
      <p className="text-sm text-fg-muted">
        You have no lists yet.{" "}
        <Link to="/lists" className="font-medium text-accent hover:underline">
          Create one
        </Link>
        .
      </p>
    );
  }
  return (
    <ul className="flex max-h-80 flex-col gap-1 overflow-y-auto">
      {manual.map((list) => (
        <li key={list.id}>
          <Button
            variant="ghost"
            className="w-full justify-start"
            loading={add.isPending && add.variables?.id === list.id}
            disabled={add.isPending}
            onClick={() =>
              add.mutate({ id: list.id, data: { ref: title.ref } })
            }
          >
            <ListPlus aria-hidden className="size-4" />
            <span className="truncate">{list.name}</span>
            {list.item_count != null && (
              <span className="ml-auto text-xs text-fg-subtle">
                {list.item_count}
              </span>
            )}
          </Button>
        </li>
      ))}
    </ul>
  );
}

function nameOf(lists: readonly List[] | undefined, id: string): string {
  return lists?.find((list) => list.id === id)?.name ?? "the list";
}
