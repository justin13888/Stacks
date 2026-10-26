import type { ReactNode } from "react";
import { Button } from "./button";
import { Dialog, DialogClose, DialogContent } from "./dialog";

export interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  /** What happens, in words the user can weigh before agreeing. */
  description: string;
  /** The confirming button's label, e.g. "Revoke token". */
  confirmLabel: string;
  /** `danger` for anything that takes something away. */
  tone?: "danger" | "primary";
  pending?: boolean;
  /** Hold the confirming button until the dialog's own fields are right. */
  confirmDisabled?: boolean;
  onConfirm: () => void;
  children?: ReactNode;
}

/**
 * Asks before an action that takes something away (revoke, deactivate,
 * delete). Cancel has the initial focus, so Enter never confirms by accident.
 */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  tone = "danger",
  pending = false,
  confirmDisabled = false,
  onConfirm,
  children,
}: ConfirmDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={title} description={description} hideClose>
        {children}
        <div className="flex flex-wrap justify-end gap-2">
          <DialogClose asChild>
            <Button variant="ghost" autoFocus>
              Cancel
            </Button>
          </DialogClose>
          <Button
            variant={tone}
            loading={pending}
            disabled={confirmDisabled}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
