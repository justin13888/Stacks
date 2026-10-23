import { Link } from "@tanstack/react-router";
import { Menu, Search } from "lucide-react";
import type { User } from "@/api/schemas";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetTrigger,
} from "@/components/ui/sheet";
import { cn } from "@/lib/cn";
import { visibleNavItems } from "./nav-items";
import { navLinkClass } from "./sidebar";
import { UserMenu } from "./user-menu";

export interface BottomNavProps {
  user: User;
  onOpenSearch: () => void;
}

const tabClass =
  "flex flex-1 flex-col items-center justify-center gap-0.5 py-2 text-2xs font-medium text-fg-subtle " +
  "transition-colors focus-visible:outline-2 focus-visible:outline-ring";

/** The mobile tab bar (below md): the main sections, search and "More". */
export function BottomNav({ user, onOpenSearch }: BottomNavProps) {
  const items = visibleNavItems(user.role);
  const primary = items.filter((item) => item.primary);
  const rest = items.filter((item) => !item.primary);

  return (
    <nav
      aria-label="Main"
      className={cn(
        "fixed inset-x-0 bottom-0 z-30 flex h-bottom-nav items-stretch border-t border-line bg-surface/95 backdrop-blur",
        "pb-[env(safe-area-inset-bottom)] md:hidden",
      )}
    >
      {primary.map(({ to, label, icon: Icon }) => (
        <Link
          key={to}
          to={to}
          activeOptions={{ exact: to === "/" }}
          className={tabClass}
          activeProps={{ className: "text-accent", "aria-current": "page" }}
        >
          <Icon aria-hidden className="size-5" />
          {label}
        </Link>
      ))}
      <button type="button" onClick={onOpenSearch} className={tabClass}>
        <Search aria-hidden className="size-5" />
        Search
      </button>
      <Sheet>
        <SheetTrigger className={tabClass}>
          <Menu aria-hidden className="size-5" />
          More
        </SheetTrigger>
        <SheetContent side="bottom" title="More">
          <nav aria-label="More sections" className="flex flex-col gap-1">
            {rest.map(({ to, label, icon: Icon }) => (
              <SheetClose key={to} asChild>
                <Link
                  to={to}
                  className={navLinkClass}
                  activeProps={{
                    className: "bg-accent-soft text-fg",
                    "aria-current": "page",
                  }}
                >
                  <Icon aria-hidden className="size-5" />
                  {label}
                </Link>
              </SheetClose>
            ))}
          </nav>
          <UserMenu user={user} />
        </SheetContent>
      </Sheet>
    </nav>
  );
}
