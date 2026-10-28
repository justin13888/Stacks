// Permanently deleting an account: only a deactivated one, never your own,
// and only once its name is typed. Driven from the keyboard alone.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { AdminUser } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import {
  json,
  makeSession,
  makeUser,
  mockApi,
  renderApp,
  route,
} from "@/test/utils";

const ME = makeUser({ role: "admin" });

const ACTIVE: AdminUser = {
  id: "0190a8e0-0000-7000-8000-000000000002",
  display_name: "Grace Hopper",
  email: "grace@example.com",
  role: "user",
  deactivated_at: null,
  created_at: "2026-02-01T00:00:00Z",
};

const LEAVING: AdminUser = {
  ...ACTIVE,
  id: "0190a8e0-0000-7000-8000-000000000003",
  display_name: "Alan Turing",
  email: "alan@example.com",
  deactivated_at: "2026-10-20T12:00:00Z",
};

function purgeServer() {
  const deletes: string[] = [];
  let users = [{ ...ME, deactivated_at: null }, ACTIVE, LEAVING];
  mockApi(async (request) => {
    const url = new URL(request.url);
    switch (route(request)) {
      case "GET /api/v1/sessions/current":
        return json(makeSession(ME));
      case "GET /api/v1/admin/users":
        return json({ items: users, next_cursor: null });
      case `DELETE /api/v1/admin/users/${LEAVING.id}`:
        deletes.push(`${url.pathname}${url.search}`);
        users = users.filter((u) => u.id !== LEAVING.id);
        return new Response(null, { status: 204 });
    }
    return undefined;
  });
  return { deletes };
}

describe("purging an account", () => {
  test("only a deactivated account other than yours offers it", async () => {
    purgeServer();
    renderApp("/admin");
    expect(
      await screen.findByRole(
        "button",
        { name: "Delete Alan Turing permanently" },
        { timeout: 5000 },
      ),
    ).toBeInTheDocument();
    for (const name of [ACTIVE.display_name, ME.display_name]) {
      expect(
        screen.queryByRole("button", { name: `Delete ${name} permanently` }),
      ).not.toBeInTheDocument();
    }
  });

  test("the name must be typed, and the keyboard is enough", async () => {
    const { deletes } = purgeServer();
    renderApp("/admin");
    const trigger = await screen.findByRole(
      "button",
      { name: "Delete Alan Turing permanently" },
      { timeout: 5000 },
    );
    trigger.focus();
    await userEvent.keyboard("{Enter}");
    const dialog = await screen.findByRole("dialog", {
      name: "Permanently delete Alan Turing?",
    });
    expect(dialog).toHaveTextContent("can't be undone");
    const field = within(dialog).getByLabelText("Type Alan Turing to confirm");
    const confirm = within(dialog).getByRole("button", {
      name: "Delete permanently",
    });
    expect(confirm).toBeDisabled();

    // Tab reaches the field; a wrong name holds the confirmation back, and
    // Enter does nothing until the name is right.
    for (let i = 0; i < 4 && document.activeElement !== field; i++)
      await userEvent.tab();
    expect(field).toHaveFocus();
    await userEvent.keyboard("Alan{Enter}");
    expect(confirm).toBeDisabled();
    expect(deletes).toEqual([]);
    await userEvent.keyboard(" Turing{Enter}");
    await waitFor(() =>
      expect(deletes).toEqual([
        `/api/v1/admin/users/${LEAVING.id}?confirm=${LEAVING.id}`,
      ]),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", {
          name: "Permanently delete Alan Turing?",
        }),
      ).not.toBeInTheDocument(),
    );
    await waitFor(() =>
      expect(screen.queryByText("Alan Turing")).not.toBeInTheDocument(),
    );
  });

  test("the dialog has no serious accessibility violations", async () => {
    purgeServer();
    renderApp("/admin");
    await userEvent.click(
      await screen.findByRole(
        "button",
        { name: "Delete Alan Turing permanently" },
        { timeout: 5000 },
      ),
    );
    await screen.findByRole("dialog", {
      name: "Permanently delete Alan Turing?",
    });
    await expectNoSeriousA11yViolations(document.body);
  });
});
