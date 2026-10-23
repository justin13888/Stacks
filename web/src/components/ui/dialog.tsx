import { X } from "lucide-react";
import { Dialog as DialogPrimitive, VisuallyHidden } from "radix-ui";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

// Radix Dialog: focus is trapped while open and returned to the trigger on
// close, Escape and an outside click dismiss, and the content is labelled by
// its title and described by its description.

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;

export const overlayClass =
  "fixed inset-0 z-40 bg-overlay backdrop-blur-[2px] data-[state=open]:animate-fade-in";

export interface DialogContentProps {
  /** Names the dialog; required for assistive technology. */
  title: string;
  /** Hide the title visually (it still names the dialog). */
  hideTitle?: boolean;
  description?: string;
  hideDescription?: boolean;
  children: ReactNode;
  /** Omit the corner close button (e.g. the palette closes on Escape). */
  hideClose?: boolean;
  className?: string;
  onOpenAutoFocus?: (event: Event) => void;
}

export function DialogContent({
  title,
  hideTitle = false,
  description,
  hideDescription = false,
  children,
  hideClose = false,
  className,
  onOpenAutoFocus,
}: DialogContentProps) {
  const titleNode = (
    <DialogPrimitive.Title className="text-lg font-semibold text-fg">
      {title}
    </DialogPrimitive.Title>
  );
  const descriptionNode = description ? (
    <DialogPrimitive.Description className="text-sm text-fg-muted">
      {description}
    </DialogPrimitive.Description>
  ) : null;

  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className={overlayClass} />
      <DialogPrimitive.Content
        onOpenAutoFocus={onOpenAutoFocus}
        // Without a description Radix warns unless told there is none.
        {...(description ? {} : { "aria-describedby": undefined })}
        className={cn(
          "fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] w-[min(92vw,32rem)] -translate-x-1/2 -translate-y-1/2",
          "flex-col gap-4 overflow-hidden rounded-xl border border-line bg-surface p-6 text-fg shadow-overlay",
          "data-[state=open]:animate-fade-in focus:outline-none",
          className,
        )}
      >
        {(!hideTitle || (descriptionNode && !hideDescription)) && (
          <div className="flex flex-col gap-1 pr-8">
            {!hideTitle && titleNode}
            {!hideDescription && descriptionNode}
          </div>
        )}
        {hideTitle && <VisuallyHidden.Root>{titleNode}</VisuallyHidden.Root>}
        {hideDescription && descriptionNode && (
          <VisuallyHidden.Root>{descriptionNode}</VisuallyHidden.Root>
        )}
        {children}
        {!hideClose && (
          <DialogPrimitive.Close
            aria-label="Close"
            className={cn(
              "absolute top-4 right-4 inline-flex size-8 items-center justify-center rounded-sm",
              "text-fg-muted transition-colors hover:bg-surface-raised hover:text-fg",
            )}
          >
            <X aria-hidden className="size-4" />
          </DialogPrimitive.Close>
        )}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
