import {
  Link,
  Outlet,
  useRouteContext,
  useRouterState,
} from "@tanstack/react-router";
import { Search } from "lucide-react";
import { useEffect, useState } from "react";
import { CommandPalette } from "@/components/search/command-palette";
import { IconButton } from "@/components/ui/button";
import { useCurrentUser } from "@/hooks/use-auth";
import { useTheme } from "@/hooks/use-theme";
import { BottomNav } from "./bottom-nav";
import { Logo } from "./logo";
import { PageTransition } from "./page-transition";
import { Sidebar } from "./sidebar";
import { StaleBanner } from "./stale-banner";

function isApplePlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);
}

/** ⌘K on Apple platforms, Ctrl+K elsewhere; either works everywhere. */
export function isPaletteShortcut(event: KeyboardEvent): boolean {
  return (
    (event.metaKey || event.ctrlKey) &&
    !event.altKey &&
    !event.shiftKey &&
    event.key.toLowerCase() === "k"
  );
}

/** The signed-in layout: navigation, the stale banner, the palette, the page. */
export function AppShell() {
  const context = useRouteContext({ from: "/_authenticated" });
  const user = useCurrentUser() ?? context.user;
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const [paletteOpen, setPaletteOpen] = useState(false);
  const shortcut = isApplePlatform() ? "⌘K" : "Ctrl K";

  // Keeps <html data-theme> in step with the account's preference.
  useTheme();

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (!isPaletteShortcut(event)) return;
      event.preventDefault();
      setPaletteOpen((open) => !open);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const openSearch = () => setPaletteOpen(true);

  return (
    <div data-testid="app-shell" className="min-h-dvh bg-canvas">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-md focus:bg-surface focus:px-3 focus:py-2"
      >
        Skip to content
      </a>
      <Sidebar
        user={user}
        onOpenSearch={openSearch}
        searchShortcut={shortcut}
      />

      <div className="flex min-h-dvh flex-col pb-bottom-nav md:pb-0 md:pl-sidebar">
        <header className="sticky top-0 z-20 flex h-14 items-center justify-between border-b border-line bg-surface/90 px-4 backdrop-blur md:hidden">
          <Link to="/" aria-label="Stacks home">
            <Logo />
          </Link>
          <IconButton
            label="Search titles"
            icon={<Search className="size-5" />}
            onClick={openSearch}
          />
        </header>
        <StaleBanner freshness={context.freshness} />
        <main
          id="main"
          className="mx-auto w-full max-w-7xl flex-1 px-4 py-6 md:px-8 md:py-8"
        >
          <PageTransition routeKey={pathname}>
            <Outlet />
          </PageTransition>
        </main>
      </div>

      <BottomNav user={user} onOpenSearch={openSearch} />
      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} />
    </div>
  );
}
