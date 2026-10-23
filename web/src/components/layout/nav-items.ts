import {
  BarChart3,
  Compass,
  History,
  House,
  Library,
  ListVideo,
  type LucideIcon,
  Settings,
  ShieldCheck,
} from "lucide-react";

export interface NavItem {
  to:
    | "/"
    | "/library"
    | "/discover"
    | "/lists"
    | "/history"
    | "/stats"
    | "/settings"
    | "/admin";
  label: string;
  icon: LucideIcon;
  /** Shown only to administrators. */
  adminOnly?: boolean;
  /** Pinned to the mobile bottom bar; the rest live under "More". */
  primary?: boolean;
}

export const NAV_ITEMS: readonly NavItem[] = [
  { to: "/", label: "Up Next", icon: House, primary: true },
  { to: "/library", label: "Library", icon: Library, primary: true },
  { to: "/discover", label: "Discover", icon: Compass, primary: true },
  { to: "/lists", label: "Lists", icon: ListVideo },
  { to: "/history", label: "History", icon: History },
  { to: "/stats", label: "Stats", icon: BarChart3 },
  { to: "/settings", label: "Settings", icon: Settings },
  { to: "/admin", label: "Admin", icon: ShieldCheck, adminOnly: true },
];

/** The sections a user with `role` may see. */
export function visibleNavItems(role: string | undefined): readonly NavItem[] {
  return NAV_ITEMS.filter((item) => !item.adminOnly || role === "admin");
}
