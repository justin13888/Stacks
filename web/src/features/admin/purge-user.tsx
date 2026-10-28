import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { getListUsersQueryKey, usePurgeUser } from "@/api/admin/admin";
import type { AdminUser } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { reportFailure } from "@/components/ui/error-state";
import { Input } from "@/components/ui/input";
import { toast } from "@/components/ui/toast";

/**
 * The danger zone of a deactivated account: delete it and everything it
 * owns, for good. The admin types the account's name before the dialog lets
 * them confirm; the request itself repeats the account's id, as the API
 * requires.
 */
export function PurgeUser({ user }: { user: AdminUser }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState("");
  const name = user.display_name;
  const confirmed = typed.trim() === name.trim();
  const purge = usePurgeUser({
    mutation: {
      onSuccess: () => {
        setOpen(false);
        toast({ tone: "success", title: `Deleted ${name}` });
      },
      onError: (error) => {
        setOpen(false);
        reportFailure(error);
      },
      onSettled: () =>
        queryClient.invalidateQueries({ queryKey: getListUsersQueryKey() }),
    },
  });
  const submit = () => {
    if (confirmed && !purge.isPending)
      purge.mutate({ id: user.id, params: { confirm: user.id } });
  };
  return (
    <>
      <Button
        variant="danger"
        size="sm"
        aria-label={`Delete ${name} permanently`}
        onClick={() => {
          setTyped("");
          setOpen(true);
        }}
      >
        Delete permanently
      </Button>
      <ConfirmDialog
        open={open}
        onOpenChange={setOpen}
        title={`Permanently delete ${name}?`}
        description={`Their library, history, lists, tags, imports, notification channels, sessions and API tokens are deleted for good. This can't be undone.`}
        confirmLabel="Delete permanently"
        confirmDisabled={!confirmed}
        pending={purge.isPending}
        onConfirm={submit}
      >
        <form
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <Input
            label={`Type ${name} to confirm`}
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
        </form>
      </ConfirmDialog>
    </>
  );
}
