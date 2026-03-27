import { Outlet, createFileRoute, redirect } from "@tanstack/react-router";
import { isAuthenticated } from "@/lib/auth";
import Sidebar from "@/components/layout/sidebar";
import { useState } from "react";

export const Route = createFileRoute("/_authenticated")({
  beforeLoad() {
    if (!isAuthenticated()) {
      throw redirect({ to: "/login" });
    }
  },
  component: AuthenticatedLayout,
});

function AuthenticatedLayout() {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  return (
    <div data-testid="authenticated-layout" className="authenticated-layout">
      {/* App shell: sidebar + main content area */}
      <Sidebar
        collapsed={sidebarCollapsed}
        onToggle={() => setSidebarCollapsed((c) => !c)}
      />
      <main className="main-content">
        <Outlet />
      </main>
    </div>
  );
}
