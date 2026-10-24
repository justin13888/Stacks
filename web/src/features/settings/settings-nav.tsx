import { Link } from "@tanstack/react-router";
import { cn } from "@/lib/cn";

const SECTIONS = [
  { to: "/settings", label: "Account" },
  { to: "/settings/import", label: "Import" },
  { to: "/settings/data", label: "Backup & export" },
  { to: "/settings/trash", label: "Trash" },
  { to: "/blocklist", label: "Not interested" },
] as const;

/** The settings sections, shown at the top of each one. */
export function SettingsNav() {
  return (
    <nav aria-label="Settings sections" className="mb-6 -mx-1 overflow-x-auto">
      <ul className="inline-flex min-w-max items-center gap-1 rounded-lg bg-surface-sunken p-1">
        {SECTIONS.map(({ to, label }) => (
          <li key={to}>
            <Link
              to={to}
              activeOptions={{ exact: true }}
              className={cn(
                "inline-flex h-8 items-center rounded-md px-3 text-sm font-medium text-fg-muted",
                "transition-colors hover:text-fg focus-visible:outline-2 focus-visible:outline-ring",
                "aria-[current=page]:bg-surface aria-[current=page]:text-fg aria-[current=page]:shadow-raised",
              )}
            >
              {label}
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  );
}
