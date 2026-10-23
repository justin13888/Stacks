// The signed-in shell: navigation for every section, Admin only for admins,
// the error boundary, and reduced motion.

import { render, screen, within } from "@testing-library/react";
import { describe, expect, test } from "vitest";
import { setMediaQueries } from "@/test/setup";
import {
  json,
  makeSession,
  makeUser,
  mockApi,
  problem,
  renderApp,
  route,
} from "@/test/utils";
import { PageTransition } from "./page-transition";

function signedInAs(role: "user" | "admin") {
  mockApi((request) =>
    route(request) === "GET /api/v1/sessions/current"
      ? json(makeSession(makeUser({ role })))
      : problem(404),
  );
}

const SECTIONS = [
  "Up Next",
  "Library",
  "Discover",
  "Lists",
  "History",
  "Stats",
  "Settings",
];

describe("app shell", () => {
  test("the sidebar links every section and marks the current page", async () => {
    signedInAs("user");
    renderApp("/library");
    await screen.findByTestId("app-shell");
    const [sidebar] = screen.getAllByRole("navigation", { name: "Main" });
    const links = within(sidebar as HTMLElement).getAllByRole("link");
    expect(links.map((l) => l.textContent)).toEqual(SECTIONS);
    expect(
      within(sidebar as HTMLElement).getByRole("link", { name: "Library" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.getByRole("heading", { level: 1, name: "Library" }),
    ).toBeInTheDocument();
  });

  test("mobile has a bottom bar with the main sections, search and More", async () => {
    signedInAs("user");
    renderApp("/");
    await screen.findByTestId("app-shell");
    const [, bottom] = screen.getAllByRole("navigation", { name: "Main" });
    const bar = within(bottom as HTMLElement);
    expect(bar.getAllByRole("link").map((l) => l.textContent)).toEqual([
      "Up Next",
      "Library",
      "Discover",
    ]);
    expect(bar.getByRole("button", { name: "Search" })).toBeInTheDocument();
    expect(bar.getByRole("button", { name: "More" })).toBeInTheDocument();
  });

  test("Admin is shown and reachable only for administrators", async () => {
    signedInAs("user");
    const user = renderApp("/admin");
    await screen.findByTestId("app-shell");
    expect(
      screen.queryByRole("link", { name: "Admin" }),
    ).not.toBeInTheDocument();
    expect(await screen.findByText("Nothing here")).toBeInTheDocument();
    user.unmount();

    signedInAs("admin");
    renderApp("/admin");
    await screen.findByTestId("app-shell");
    expect(
      screen.getAllByRole("link", { name: "Admin" }).length,
    ).toBeGreaterThan(0);
    expect(
      screen.getByRole("heading", { level: 1, name: "Admin" }),
    ).toBeInTheDocument();
  });

  test("an API outage while checking the session lands on the error boundary", async () => {
    mockApi(() => problem(500, "internal"));
    renderApp("/");
    const alert = await screen.findByRole("alert");
    expect(
      within(alert).getByRole("button", { name: "Try again" }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("app-shell")).not.toBeInTheDocument();
  });
});

describe("reduced motion", () => {
  test("pages animate in by default", () => {
    render(<PageTransition routeKey="/">Page</PageTransition>);
    expect(screen.getByText("Page")).toHaveAttribute("data-motion", "on");
  });

  test("prefers-reduced-motion renders pages with no animation", () => {
    setMediaQueries(["(prefers-reduced-motion: reduce)"]);
    render(<PageTransition routeKey="/">Page</PageTransition>);
    const page = screen.getByText("Page");
    expect(page).toHaveAttribute("data-motion", "off");
    expect(page.getAttribute("style") ?? "").not.toMatch(/transform|opacity/);
  });

  test("the stylesheet collapses CSS animation under the same preference", async () => {
    const { readFileSync } = await import("node:fs");
    const { join } = await import("node:path");
    const css = readFileSync(
      join(import.meta.dirname, "../../styles/global.css"),
      "utf8",
    );
    const block = css.slice(
      css.indexOf("@media (prefers-reduced-motion: reduce)"),
    );
    expect(block).toMatch(/animation-duration:\s*0\.01ms !important/);
    expect(block).toMatch(/transition-duration:\s*0\.01ms !important/);
  });
});
