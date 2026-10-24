import type { ReactNode } from "react";
import { useId } from "react";

/** One titled block of a settings page. */
export function SettingsSection({
  title,
  description,
  actions,
  children,
}: {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
}) {
  const headingId = useId();
  return (
    <section
      aria-labelledby={headingId}
      className="flex flex-col gap-4 rounded-xl border border-line bg-surface p-4 sm:p-6"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 id={headingId} className="text-lg font-semibold text-fg">
            {title}
          </h2>
          {description && (
            <p className="text-sm text-fg-muted">{description}</p>
          )}
        </div>
        {actions}
      </div>
      {children}
    </section>
  );
}

/** A short "label: value" line under a row's name. */
export function Meta({ children }: { children: ReactNode }) {
  return <span className="text-xs text-fg-muted">{children}</span>;
}
