// Cookie sign-in end to end against a mocked API: the login redirect, the
// callback's code exchange, the route guard, sign-out, and a session that
// ends mid-use. No credential is ever stored by the app.

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import {
  json,
  makeSession,
  makeUser,
  mockApi,
  problem,
  renderApp,
  requestsOf,
  route,
  testQueryClient,
} from "@/test/utils";
import {
  browserNavigation,
  exchangeAuthorizationCode,
  sessionQueryOptions,
} from "./use-auth";

describe("session query", () => {
  test("a 401 is an answer: signed out, cached as null", async () => {
    mockApi(() => problem(401));
    const client = testQueryClient();
    await expect(client.fetchQuery(sessionQueryOptions)).resolves.toBeNull();
  });

  test("any other failure stays an error", async () => {
    mockApi(() => problem(500, "internal"));
    const client = testQueryClient();
    await expect(client.fetchQuery(sessionQueryOptions)).rejects.toMatchObject({
      status: 500,
    });
  });
});

describe("sign-in", () => {
  test("a signed-out visitor is sent to sign in, which redirects to the provider", async () => {
    const assign = vi
      .spyOn(browserNavigation, "assign")
      .mockImplementation(() => {});
    const api = mockApi((request) => {
      switch (route(request)) {
        case "GET /api/v1/sessions/current":
          return problem(401);
        case "POST /api/v1/auth/oidc/authorizations":
          return json({
            authorization_url: "https://idp.example/authorize?state=s1",
          });
      }
    });
    const { router } = renderApp("/library");

    await screen.findByTestId("login-page");
    expect(router.state.location.pathname).toBe("/login");
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));

    await waitFor(() =>
      expect(assign).toHaveBeenCalledWith(
        "https://idp.example/authorize?state=s1",
      ),
    );
    expect(requestsOf(api)).toContain("POST /api/v1/auth/oidc/authorizations");
  });

  test("the callback posts code and state once, then lands in the app", async () => {
    let signedIn = false;
    const bodies: unknown[] = [];
    mockApi(async (request) => {
      switch (route(request)) {
        case "POST /api/v1/sessions":
          bodies.push(await request.json());
          signedIn = true;
          return json(makeSession(), 201);
        case "GET /api/v1/sessions/current":
          return signedIn ? json(makeSession()) : problem(401);
      }
    });
    const { router } = renderApp("/auth/callback?code=abc&state=xyz");

    await screen.findByTestId("app-shell");
    expect(router.state.location.pathname).toBe("/");
    expect(bodies).toEqual([{ code: "abc", state: "xyz" }]);
  });

  test("the same code is never spent twice", async () => {
    const api = mockApi(() => json(makeSession(), 201));
    const [a, b] = await Promise.all([
      exchangeAuthorizationCode("once", "s"),
      exchangeAuthorizationCode("once", "s"),
    ]);
    expect(a).toEqual(b);
    expect(requestsOf(api)).toEqual(["POST /api/v1/sessions"]);
  });

  test("a refused sign-in explains itself and links back", async () => {
    mockApi((request) =>
      route(request) === "POST /api/v1/sessions"
        ? problem(400, "sign-in-failed")
        : problem(401),
    );
    renderApp("/auth/callback?code=bad&state=xyz");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "Back to sign in" }),
    ).toHaveAttribute("href", "/login");
  });

  test("a provider error in the redirect is shown without calling the API", async () => {
    const api = mockApi(() => problem(401));
    renderApp(
      "/auth/callback?error=access_denied&error_description=You+said+no",
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("You said no");
    expect(requestsOf(api)).not.toContain("POST /api/v1/sessions");
  });
});

describe("signed in", () => {
  test("sign-out deletes the session and returns to sign in", async () => {
    let signedIn = true;
    const api = mockApi((request) => {
      switch (route(request)) {
        case "GET /api/v1/sessions/current":
          return signedIn ? json(makeSession()) : problem(401);
        case "DELETE /api/v1/sessions/current":
          signedIn = false;
          return new Response(null, { status: 204 });
      }
    });
    const { router } = renderApp("/");
    const [menu] = await screen.findAllByRole("button", {
      name: /Account menu/,
    });
    await userEvent.click(menu as HTMLElement);
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Sign out" }),
    );

    await screen.findByTestId("login-page");
    expect(router.state.location.pathname).toBe("/login");
    expect(requestsOf(api)).toContain("DELETE /api/v1/sessions/current");
  });

  test("a 401 from any call mid-use signs the user out", async () => {
    mockApi((request) => {
      switch (route(request)) {
        case "GET /api/v1/sessions/current":
          return json(makeSession(makeUser()));
        case "GET /api/v1/search":
          return problem(401);
      }
    });
    const { router } = renderApp("/");
    await screen.findByTestId("app-shell");
    await userEvent.keyboard("{Control>}k{/Control}");
    await userEvent.type(
      await screen.findByRole("combobox", { name: "Search titles" }),
      "dune",
    );

    await screen.findByTestId("login-page", undefined, { timeout: 3000 });
    expect(router.state.location.pathname).toBe("/login");
  });

  test("no credential is written to storage", async () => {
    mockApi((request) =>
      route(request) === "GET /api/v1/sessions/current"
        ? json(makeSession())
        : undefined,
    );
    renderApp("/");
    await screen.findByTestId("app-shell");
    const keys = Object.keys(window.localStorage).concat(
      Object.keys(window.sessionStorage),
    );
    expect(keys.filter((k) => /token|session|auth/i.test(k))).toEqual([]);
  });
});
