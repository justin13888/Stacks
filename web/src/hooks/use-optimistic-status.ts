import { useMutation, useQueryClient } from "@tanstack/react-query";

interface WatchStatus {
  cameo_media_id: string;
  status: string;
  score: number | null;
  is_favorite: boolean;
}

/**
 * Optimistic watch status update hook.
 * Immediately updates the cached media detail while the API call is in-flight.
 */
export function useOptimisticStatus(cameoMediaId: string) {
  const queryClient = useQueryClient();
  const queryKey = ["media", cameoMediaId, "status"];

  return useMutation({
    mutationFn: async (update: Partial<WatchStatus>) => {
      const res = await fetch(`/api/v1/users/me/media/${cameoMediaId}/status`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(update),
      });
      if (!res.ok) throw new Error("Failed to update status");
      return res.json() as Promise<WatchStatus>;
    },
    onMutate: async (update) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<WatchStatus>(queryKey);
      queryClient.setQueryData<WatchStatus>(queryKey, (old) =>
        old ? { ...old, ...update } : (update as WatchStatus),
      );
      return { previous };
    },
    onError: (_err, _update, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKey, context.previous);
      }
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey });
    },
  });
}
