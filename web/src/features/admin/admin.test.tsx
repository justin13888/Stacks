// Admin in the real app: only administrators reach it, and every change to
// an account is confirmed first.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type { AdminUser, Job } from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/test/axe";
import {
  json,
  makeSession,
  makeUser,
  mockApi,
  renderApp,
  requestsOf,
  route,
} from "@/test/utils";

const ME = makeUser({ role: "admin" });

const GRACE: AdminUser = {
  id: "0190a8e0-0000-7000-8000-000000000002",
  display_name: "Grace Hopper",
  email: "grace@example.com",
  role: "user",
  deactivated_at: null,
  created_at: "2026-02-01T00:00:00Z",
};

const FAILED: Job = {
  id: "0190a8e0-0000-7000-8000-0000000000j1",
  type: "import",
  status: "failed",
  attempts: 5,
  max_attempts: 5,
  run_at: "2026-10-20T00:00:00Z",
  created_at: "2026-10-19T00:00:00Z",
  completed_at: "2026-10-20T00:05:00Z",
  error: "TMDB answered 503",
};

const DONE: Job = {
  ...FAILED,
  id: "0190a8e0-0000-7000-8000-0000000000j2",
  type: "notification",
  status: "completed",
  attempts: 1,
  error: null,
};

interface Write {
  method: string;
  path: string;
  body: unknown;
}

function adminServer(role: "admin" | "user" = "admin") {
  const writes: Write[] = [];
  let grace = GRACE;
  let jobs = [FAILED, DONE];
  const api = mockApi(async (request) => {
    const url = new URL(request.url);
    const key = route(request);
    if (request.method !== "GET") {
      const text = await request.text();
      writes.push({
        method: request.method,
        path: url.pathname.replace(/^\/api\/v1/, ""),
        body: text ? JSON.parse(text) : undefined,
      });
    }
    switch (key) {
      case "GET /api/v1/sessions/current":
        return json(makeSession({ ...ME, role }));
      case "GET /api/v1/admin/users":
        return json({
          items: [{ ...ME, deactivated_at: null }, grace],
          next_cursor: null,
        });
      case `PATCH /api/v1/admin/users/${GRACE.id}`: {
        const patch = writes.at(-1)?.body as {
          role?: "admin" | "user";
          deactivated?: boolean;
        };
        grace = {
          ...grace,
          ...(patch.role ? { role: patch.role } : {}),
          ...(patch.deactivated !== undefined
            ? {
                deactivated_at: patch.deactivated
                  ? "2026-10-20T12:00:00Z"
                  : null,
              }
            : {}),
        };
        return json(grace);
      }
      case "GET /api/v1/admin/jobs": {
        const status = url.searchParams.get("status");
        const type = url.searchParams.get("type");
        return json({
          items: jobs.filter(
            (j) =>
              (!status || j.status === status) && (!type || j.type === type),
          ),
          next_cursor: null,
        });
      }
      case `POST /api/v1/admin/jobs/${FAILED.id}/retries`:
        jobs = jobs.map((j) =>
          j.id === FAILED.id ? { ...j, status: "pending", attempts: 0 } : j,
        );
        return json(jobs[0], 201);
    }
    return undefined;
  });
  return { api, writes };
}

describe("admin", () => {
  test("a non-admin can't reach it, and nothing admin-only is asked for", async () => {
    const { api } = adminServer("user");
    renderApp("/admin");
    expect(
      await screen.findByRole(
        "heading",
        { name: "Nothing here" },
        { timeout: 5000 },
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("link", { name: "Admin" }),
    ).not.toBeInTheDocument();
    expect(requestsOf(api).some((r) => r.includes("/admin/"))).toBe(false);
  });

  test("an admin changes a role only after confirming it", async () => {
    const { writes } = adminServer();
    renderApp("/admin");
    const role = await screen.findByRole(
      "combobox",
      { name: "Role of Grace Hopper" },
      { timeout: 5000 },
    );
    // Your own role and account are out of reach.
    expect(
      screen.getByRole("combobox", { name: `Role of ${ME.display_name}` }),
    ).toBeDisabled();

    await userEvent.click(role);
    await userEvent.click(await screen.findByRole("option", { name: "Admin" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Make Grace Hopper an admin?",
    });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Cancel" }),
    );
    expect(writes).toEqual([]);

    await userEvent.click(role);
    await userEvent.click(await screen.findByRole("option", { name: "Admin" }));
    await userEvent.click(
      await screen.findByRole("button", { name: "Make admin" }),
    );
    await waitFor(() =>
      expect(writes).toEqual([
        {
          method: "PATCH",
          path: `/admin/users/${GRACE.id}`,
          body: { role: "admin" },
        },
      ]),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("combobox", { name: "Role of Grace Hopper" }),
      ).toHaveTextContent("Admin"),
    );
  });

  test("an admin deactivates a user after confirming, and can reactivate them", async () => {
    const { writes } = adminServer();
    renderApp("/admin");
    await userEvent.click(
      await screen.findByRole(
        "button",
        { name: "Deactivate Grace Hopper" },
        { timeout: 5000 },
      ),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Deactivate Grace Hopper?",
    });
    expect(dialog).toHaveTextContent("Their data is kept");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Deactivate" }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Reactivate Grace Hopper" }),
    );
    await userEvent.click(
      within(
        await screen.findByRole("dialog", { name: "Reactivate Grace Hopper?" }),
      ).getByRole("button", { name: "Reactivate" }),
    );
    await waitFor(() =>
      expect(writes.map((w) => w.body)).toEqual([
        { deactivated: true },
        { deactivated: false },
      ]),
    );
  });

  test("a failed job shows its error and can be retried; filters narrow the list", async () => {
    const { api, writes } = adminServer();
    renderApp("/admin");
    await userEvent.click(
      await screen.findByRole("tab", { name: "Jobs" }, { timeout: 5000 }),
    );
    expect(await screen.findByText("TMDB answered 503")).toBeInTheDocument();
    expect(screen.getByText("Notification")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("combobox", { name: "Type" }));
    await userEvent.click(
      await screen.findByRole("option", { name: "Import" }),
    );
    // The type filter is the server's, so it reaches past the loaded pages.
    await waitFor(() =>
      expect(requestsOf(api)).toContain(
        "GET /api/v1/admin/jobs?type=import&limit=50",
      ),
    );
    await waitFor(() =>
      expect(screen.queryByText("Notification")).not.toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("combobox", { name: "Status" }));
    await userEvent.click(
      await screen.findByRole("option", { name: "Failed" }),
    );
    await waitFor(() =>
      expect(requestsOf(api)).toContain(
        "GET /api/v1/admin/jobs?status=failed&type=import&limit=50",
      ),
    );

    await userEvent.click(
      await screen.findByRole("button", { name: /^Retry Import job/ }),
    );
    await waitFor(() =>
      expect(writes).toEqual([
        {
          method: "POST",
          path: `/admin/jobs/${FAILED.id}/retries`,
          body: undefined,
        },
      ]),
    );
  });

  test("the page has no serious accessibility violations", async () => {
    adminServer();
    const { container } = renderApp("/admin");
    await screen.findByRole(
      "button",
      { name: "Deactivate Grace Hopper" },
      { timeout: 5000 },
    );
    await expectNoSeriousA11yViolations(container);
  });
});
