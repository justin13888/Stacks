import { CalendarDays } from "lucide-react";
import { type ReactNode, useState } from "react";
import type { RunDate } from "@/api/schemas";
import { Button, IconButton } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent } from "@/components/ui/dialog";
import {
  formatRunDate,
  todayWhen,
  toRunDate,
  type WhenValue,
  WhenField,
  whenError,
} from "@/components/ui/when-field";

export interface WhenDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: string;
  /** The field's legend, e.g. "Watched". */
  label: string;
  /** The confirming button's label, e.g. "Mark watched". */
  confirmLabel: string;
  pending?: boolean;
  onConfirm: (date: RunDate) => void;
}

/**
 * Asks when something happened, to the day, the month or the year, and
 * starts at today. The confirm button says the date it will record.
 */
export function WhenDialog({ open, onOpenChange, ...props }: WhenDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* Mounted only while open, so each opening starts again at today. */}
      {open && <WhenForm {...props} />}
    </Dialog>
  );
}

function WhenForm({
  title,
  description,
  label,
  confirmLabel,
  pending = false,
  onConfirm,
}: Omit<WhenDialogProps, "open" | "onOpenChange">) {
  const [value, setValue] = useState<WhenValue>(() => todayWhen());
  const error = whenError(value);
  const date = toRunDate(value);
  return (
    <DialogContent title={title} description={description}>
      <form
        className="flex flex-col gap-5"
        onSubmit={(event) => {
          event.preventDefault();
          if (!error) onConfirm(date);
        }}
      >
        <WhenField
          label={label}
          value={value}
          onChange={setValue}
          error={error}
        />
        <div className="flex flex-wrap items-center justify-end gap-2">
          <DialogClose asChild>
            <Button variant="ghost">Cancel</Button>
          </DialogClose>
          <Button type="submit" loading={pending} disabled={error !== null}>
            {confirmLabel}
            {!error && (
              <span className="font-normal opacity-80">
                · {formatRunDate(date)}
              </span>
            )}
          </Button>
        </div>
      </form>
    </DialogContent>
  );
}

/**
 * An action that happens now, with a calendar button beside it that does it
 * on a date the user picks instead.
 */
export function DatedAction({
  children,
  label,
  dialog,
  onConfirm,
  pending,
}: {
  /** The undated action's button. */
  children: ReactNode;
  /** The calendar button's name, e.g. "Mark watched on a date…". */
  label: string;
  dialog: Omit<
    WhenDialogProps,
    "open" | "onOpenChange" | "onConfirm" | "pending"
  >;
  /** Does the action on `date`; resolves once it is done. */
  onConfirm: (date: RunDate) => Promise<unknown>;
  pending: boolean;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="inline-flex items-center gap-1">
      {children}
      <IconButton
        label={label}
        variant="secondary"
        icon={<CalendarDays aria-hidden className="size-4" />}
        onClick={() => setOpen(true)}
      />
      <WhenDialog
        {...dialog}
        open={open}
        onOpenChange={setOpen}
        pending={pending}
        onConfirm={(date) => {
          // A failure is reported where the action runs; the dialog stays.
          void onConfirm(date).then(
            () => setOpen(false),
            () => undefined,
          );
        }}
      />
    </div>
  );
}
