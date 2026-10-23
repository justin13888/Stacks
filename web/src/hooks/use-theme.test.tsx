// Light and dark themes, system detection, and the choice persisting to the
// account through PATCH /me so it survives a reload.

import { act, cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { Theme, UserPatch } from "@/api/schemas";
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
import { applyInitialTheme } from "./use-theme";

const DARK = "(prefers-color-scheme: dark)";

/** A fake account whose stored theme PATCH /me changes. */
function accountServer(initial: Theme) {
  let theme = initial;
  const patches: UserPatch[] = [];
  mockApi(async (request) => {
    switch (route(request)) {
      case "GET /api/v1/sessions/current":
        return json(makeSession(makeUser({}, theme)));
      case "PATCH /api/v1/me": {
        const patch = (await request.json()) as UserPatch;
        patches.push(patch);
        theme = patch.preferences?.theme ?? theme;
        return json(makeUser({}, theme));
      }
    }
    return problem(404);
  });
  return { patches, theme: () => theme };
}

const htmlTheme = () => document.documentElement.dataset.theme;

async function chooseTheme(label: string) {
  const [menu] = await screen.findAllByRole("button", { name: /Account menu/ });
  await userEvent.click(menu as HTMLElement);
  await userEvent.click(
    await screen.findByRole("menuitemradio", { name: label }),
  );
}

describe("theme", () => {
  test("the account's theme is painted on <html>", async () => {
    accountServer("dark");
    renderApp("/");
    await screen.findByTestId("app-shell");
    await waitFor(() => expect(htmlTheme()).toBe("dark"));
  });

  test("system follows the OS setting, live", async () => {
    setMediaQueries([DARK]);
    accountServer("system");
    renderApp("/");
    await screen.findByTestId("app-shell");
    await waitFor(() => expect(htmlTheme()).toBe("dark"));
    act(() => setMediaQueries([]));
    await waitFor(() => expect(htmlTheme()).toBe("light"));
  });

  test("a choice is saved to the account and survives a reload", async () => {
    const server = accountServer("system");
    renderApp("/");
    await screen.findByTestId("app-shell");
    await waitFor(() => expect(htmlTheme()).toBe("light"));

    await chooseTheme("Dark");
    // Applied at once, before the server answers.
    expect(htmlTheme()).toBe("dark");
    await waitFor(() =>
      expect(server.patches).toEqual([{ preferences: { theme: "dark" } }]),
    );
    expect(server.theme()).toBe("dark");

    // Reload: a new app, a new cache, and a different browser (no hint).
    cleanup();
    window.localStorage.clear();
    delete document.documentElement.dataset.theme;
    renderApp("/");
    await screen.findByTestId("app-shell");
    await waitFor(() => expect(htmlTheme()).toBe("dark"));
  });

  test("a failed save puts the old theme back and says so", async () => {
    mockApi((request) => {
      switch (route(request)) {
        case "GET /api/v1/sessions/current":
          return json(makeSession(makeUser({}, "light")));
        case "PATCH /api/v1/me":
          return problem(500, "internal");
      }
    });
    renderApp("/");
    await screen.findByTestId("app-shell");
    await chooseTheme("Dark");
    expect(
      await screen.findByText("Couldn't save your theme"),
    ).toBeInTheDocument();
    await waitFor(() => expect(htmlTheme()).toBe("light"));
  });

  test("the last theme is painted before the session loads", () => {
    window.localStorage.setItem("stacks.theme", "dark");
    applyInitialTheme();
    expect(htmlTheme()).toBe("dark");
  });
});
