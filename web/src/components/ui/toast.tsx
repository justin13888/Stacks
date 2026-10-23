import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import { Toast as ToastPrimitive } from "radix-ui";
import { type ReactNode, useSyncExternalStore } from "react";
import { cn } from "@/lib/cn";

// Toasts are raised with `toast()` from anywhere, React or not (the query
// error handler calls it), and rendered by the one <Toaster> in the shell on
// Radix Toast: announced through a live region, paused on hover and focus,
// swipe-dismissable, and reachable from the keyboard with F8.

export type ToastTone = "info" | "success" | "error";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastInput {
  title: string;
  description?: string;
  tone?: ToastTone;
  action?: ToastAction;
  /** Milliseconds before it closes on its own. */
  durationMs?: number;
}

interface ToastRecord extends ToastInput {
  id: string;
}

const MAX_VISIBLE = 4;

let records: readonly ToastRecord[] = [];
const listeners = new Set<() => void>();
let nextId = 0;

function emit(next: readonly ToastRecord[]) {
  records = next;
  for (const listener of listeners) listener();
}

/** Shows a toast and returns its id. */
export function toast(input: ToastInput): string {
  nextId += 1;
  const id = `toast-${nextId}`;
  emit([...records, { ...input, id }].slice(-MAX_VISIBLE));
  return id;
}

/** Closes a toast. */
export function dismissToast(id: string): void {
  emit(records.filter((r) => r.id !== id));
}

/** Clears every toast at once (tests, sign-out). */
export function clearToasts(): void {
  emit([]);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function useToasts(): readonly ToastRecord[] {
  return useSyncExternalStore(
    subscribe,
    () => records,
    () => records,
  );
}

const toneIcon = {
  info: <Info aria-hidden className="size-5 text-info" />,
  success: <CircleCheck aria-hidden className="size-5 text-success" />,
  error: <CircleAlert aria-hidden className="size-5 text-danger" />,
} satisfies Record<ToastTone, ReactNode>;

export function Toaster() {
  const toasts = useToasts();
  return (
    <ToastPrimitive.Provider swipeDirection="right" label="Notifications">
      {toasts.map((t) => {
        const tone = t.tone ?? "info";
        return (
          <ToastPrimitive.Root
            key={t.id}
            onOpenChange={(open) => {
              if (!open) dismissToast(t.id);
            }}
            duration={t.durationMs ?? (tone === "error" ? 8000 : 4000)}
            // Errors interrupt; confirmations wait their turn.
            type={tone === "error" ? "foreground" : "background"}
            data-tone={tone}
            className={cn(
              "pointer-events-auto flex w-full items-start gap-3 rounded-lg border border-line bg-surface p-4",
              "text-fg shadow-overlay data-[state=open]:animate-fade-in",
              "data-[swipe=move]:translate-x-(--radix-toast-swipe-move-x) data-[swipe=end]:translate-x-(--radix-toast-swipe-end-x)",
            )}
          >
            {toneIcon[tone]}
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <ToastPrimitive.Title className="text-sm font-semibold">
                {t.title}
              </ToastPrimitive.Title>
              {t.description && (
                <ToastPrimitive.Description className="text-sm text-fg-muted">
                  {t.description}
                </ToastPrimitive.Description>
              )}
              {t.action && (
                <ToastPrimitive.Action
                  altText={t.action.label}
                  onClick={t.action.onClick}
                  className="mt-1 self-start rounded-sm text-sm font-medium text-accent hover:underline"
                >
                  {t.action.label}
                </ToastPrimitive.Action>
              )}
            </div>
            <ToastPrimitive.Close
              aria-label="Dismiss notification"
              className="inline-flex size-6 items-center justify-center rounded-sm text-fg-muted hover:text-fg"
            >
              <X aria-hidden className="size-4" />
            </ToastPrimitive.Close>
          </ToastPrimitive.Root>
        );
      })}
      <ToastPrimitive.Viewport
        className={cn(
          "fixed right-0 bottom-(--spacing-bottom-nav) z-[60] flex w-full max-w-sm flex-col gap-2 p-4 outline-none",
          "md:bottom-0",
        )}
      />
    </ToastPrimitive.Provider>
  );
}
