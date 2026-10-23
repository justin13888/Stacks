import { Link } from "@tanstack/react-router";
import { Search } from "lucide-react";
import type { User } from "@/api/schemas";
import { cn } from "@/lib/cn";
import { Logo } from "./logo";
import { visibleNavItems } from "./nav-items";
import { UserMenu } from "./user-menu";

export interface SidebarProps {
  user: User;
  onOpenSearch: () => void;
  /** The shortcut shown on the search button, e.g. ⌘K or Ctrl K. */
  searchShortcut: string;
}

export const navLinkClass =
  "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-fg-muted " +
  "transition-colors duration-(--duration-fast) hover:bg-surface-raised hover:text-fg " +
  "focus-visible:outline-2 focus-visible:outline-ring";

/** The desktop navigation rail (md and up). */
export function Sidebar({ user, onOpenSearch, searchShortcut }: SidebarProps) {
  return (
    <aside className="fixed inset-y-0 left-0 z-30 hidden w-sidebar flex-col gap-4 border-r border-line bg-surface p-4 md:flex">
      <Link
        to="/"
        className="flex items-center gap-2 px-2 py-1"
        aria-label="Stacks home"
      >
        <Logo />
      </Link>

      <button
        type="button"
        onClick={onOpenSearch}
        className={cn(
          "flex h-10 items-center gap-2 rounded-md border border-line bg-canvas px-3 text-sm text-fg-subtle",
          "transition-colors hover:border-line-strong hover:text-fg-muted focus-visible:outline-2 focus-visible:outline-ring",
        )}
      >
        <Search aria-hidden className="size-4" />
        <span className="flex-1 text-left">Search titles</span>
        <kbd className="rounded-xs border border-line bg-surface px-1.5 font-mono text-2xs text-fg-subtle">
          {searchShortcut}
        </kbd>
      </button>

      <nav aria-label="Main" className="flex flex-1 flex-col gap-1">
        {visibleNavItems(user.role).map(({ to, label, icon: Icon }) => (
          <Link
            key={to}
            to={to}
            activeOptions={{ exact: to === "/" }}
            className={navLinkClass}
            activeProps={{
              className: "bg-accent-soft text-fg",
              "aria-current": "page",
            }}
          >
            <Icon aria-hidden className="size-5" />
            {label}
          </Link>
        ))}
      </nav>

      <UserMenu user={user} />
    </aside>
  );
}
