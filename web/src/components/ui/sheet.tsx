import { X } from "lucide-react";
import { Dialog as DialogPrimitive } from "radix-ui";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";
import { overlayClass } from "./dialog";

// A Sheet is a Dialog anchored to a screen edge: the same focus trap, Escape
// and labelling, laid out as a side panel or a bottom drawer.

export const Sheet = DialogPrimitive.Root;
export const SheetTrigger = DialogPrimitive.Trigger;
export const SheetClose = DialogPrimitive.Close;

export type SheetSide = "left" | "right" | "bottom";

const sides: Record<SheetSide, string> = {
  left: "inset-y-0 left-0 h-full w-[min(85vw,20rem)] border-r",
  right: "inset-y-0 right-0 h-full w-[min(85vw,24rem)] border-l",
  bottom: "inset-x-0 bottom-0 max-h-[85vh] rounded-t-xl border-t",
};

export interface SheetContentProps {
  title: string;
  description?: string;
  side?: SheetSide;
  children: ReactNode;
  className?: string;
}

export function SheetContent({
  title,
  description,
  side = "right",
  children,
  className,
}: SheetContentProps) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className={overlayClass} />
      <DialogPrimitive.Content
        data-side={side}
        {...(description ? {} : { "aria-describedby": undefined })}
        className={cn(
          "fixed z-50 flex flex-col gap-4 overflow-y-auto border-line bg-surface p-6 text-fg shadow-overlay",
          "data-[state=open]:animate-fade-in focus:outline-none",
          sides[side],
          className,
        )}
      >
        <div className="flex flex-col gap-1 pr-8">
          <DialogPrimitive.Title className="text-lg font-semibold">
            {title}
          </DialogPrimitive.Title>
          {description && (
            <DialogPrimitive.Description className="text-sm text-fg-muted">
              {description}
            </DialogPrimitive.Description>
          )}
        </div>
        {children}
        <DialogPrimitive.Close
          aria-label="Close"
          className="absolute top-4 right-4 inline-flex size-8 items-center justify-center rounded-sm text-fg-muted hover:bg-surface-raised hover:text-fg"
        >
          <X aria-hidden className="size-4" />
        </DialogPrimitive.Close>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
