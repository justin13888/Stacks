import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export interface EmptyStateProps {
  title: string;
  description?: ReactNode;
  /** A decorative icon above the title. */
  icon?: ReactNode;
  /** A call to action, e.g. a Button. */
  action?: ReactNode;
  className?: string;
}

/** What a view shows when it has nothing to show yet. */
export function EmptyState({
  title,
  description,
  icon,
  action,
  className,
}: EmptyStateProps) {
  return (
    <section
      aria-label={title}
      className={cn(
        "mx-auto flex max-w-md flex-col items-center gap-3 px-6 py-16 text-center",
        className,
      )}
    >
      {icon && (
        <div
          aria-hidden
          className="mb-2 flex size-14 items-center justify-center rounded-full bg-accent-soft text-accent [&_svg]:size-7"
        >
          {icon}
        </div>
      )}
      <h2 className="font-display text-lg font-semibold text-fg">{title}</h2>
      {description && <p className="text-sm text-fg-muted">{description}</p>}
      {action && <div className="mt-2">{action}</div>}
    </section>
  );
}
