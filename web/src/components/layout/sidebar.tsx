import { Link } from "@tanstack/react-router";

interface SidebarProps {
  collapsed: boolean;
  onToggle: () => void;
}

const NAV_LINKS = [
  { to: "/", label: "Home" },
  { to: "/search", label: "Search" },
  { to: "/library", label: "Library" },
  { to: "/lists", label: "Lists" },
  { to: "/stats", label: "Stats" },
  { to: "/settings", label: "Settings" },
] as const;

export default function Sidebar({ collapsed, onToggle }: SidebarProps) {
  return (
    <nav
      data-testid="sidebar"
      data-collapsed={collapsed}
      className={`sidebar ${collapsed ? "sidebar--collapsed" : ""}`}
    >
      {/* Sidebar navigation stub */}
      <button
        type="button"
        className="sidebar-toggle"
        onClick={onToggle}
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {collapsed ? "→" : "←"}
      </button>
      <ul className="sidebar-nav">
        {NAV_LINKS.map(({ to, label }) => (
          <li key={to}>
            <Link to={to} className="sidebar-nav-link">
              {!collapsed && label}
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  );
}
