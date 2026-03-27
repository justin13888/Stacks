import { Link } from "@tanstack/react-router";

const NAV_LINKS = [
  { to: "/", label: "Home" },
  { to: "/search", label: "Search" },
  { to: "/library", label: "Library" },
  { to: "/lists", label: "Lists" },
  { to: "/stats", label: "Stats" },
  { to: "/settings", label: "Settings" },
] as const;

export default function BottomBar() {
  return (
    <nav data-testid="bottom-bar" className="bottom-bar">
      {/* Mobile tab bar stub */}
      {NAV_LINKS.map(({ to, label }) => (
        <Link key={to} to={to} className="bottom-bar-tab">
          {label}
        </Link>
      ))}
    </nav>
  );
}
