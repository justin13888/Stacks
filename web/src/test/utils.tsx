// Test helpers: a fake API behind the one `fetch` the mutator calls, and
// renderers for a component, or for the whole app at a URL.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { vi } from "vitest";
import type { CurrentSession, Theme, User } from "@/api/schemas";
import { createQueryClient } from "@/config/query-client";
import { routeTree } from "@/routeTree.gen";

const ERROR_MODEL =
  "https://github.com/justin13888/Stacks/blob/master/docs/adr/0006-error-model.md#";

export function json(
  body: unknown,
  status = 200,
  headers?: HeadersInit,
): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });
}

/** An RFC 9457 problem answer, `kind` being the error model's fragment. */
export function problem(
  status: number,
  kind?: string,
  headers?: HeadersInit,
): Response {
  return new Response(
    JSON.stringify({
      type: kind ? `${ERROR_MODEL}${kind}` : "about:blank",
      status,
      title: kind ?? "Error",
    }),
    {
      status,
      headers: { "content-type": "application/problem+json", ...headers },
    },
  );
}

/**
 * Answers one request. `body` is the body as the app passed it to `fetch`:
 * a multipart body has to be read from there, because from Node 24 undici
 * does not serialise jsdom's `File` into a `Request` (the part arrives empty).
 */
export type Handler = (
  request: Request,
  body: RequestInit["body"],
) => Response | undefined | Promise<Response | undefined>;

/**
 * Replaces `fetch` with `handler`; an unhandled request is a 404 problem, so a
 * test never reaches the network. Returns the mock to inspect calls.
 */
export function mockApi(handler: Handler) {
  const mock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url =
      typeof input === "string" ? new URL(input, "http://localhost") : input;
    const request = new Request(url, init);
    return (await handler(request, init?.body)) ?? problem(404);
  });
  vi.stubGlobal("fetch", mock);
  return mock;
}

/** The requests the mock received, as `METHOD /path?query`. */
export function requestsOf(mock: ReturnType<typeof mockApi>): string[] {
  return mock.mock.calls.map(([input, init]) => {
    const url = new URL(String(input), "http://localhost");
    return `${init?.method ?? "GET"} ${url.pathname}${url.search}`;
  });
}

export function route(request: Request): string {
  const url = new URL(request.url);
  return `${request.method} ${url.pathname}`;
}

export function makeUser(
  overrides: Partial<User> = {},
  theme: Theme = "system",
): User {
  return {
    id: "0190a8e0-0000-7000-8000-000000000001",
    display_name: "Ada Lovelace",
    email: "ada@example.com",
    role: "user",
    created_at: "2026-01-01T00:00:00Z",
    preferences: {
      score_format: "point_10_decimal",
      title_language: "english",
      spoilers: "hide",
      theme,
    },
    ...overrides,
  };
}

export function makeSession(user: User = makeUser()): CurrentSession {
  return {
    user,
    session: {
      id: "0190a8e0-0000-7000-8000-0000000000aa",
      current: true,
      created_at: "2026-01-01T00:00:00Z",
      last_seen_at: "2026-01-01T00:00:00Z",
      expires_at: "2026-02-01T00:00:00Z",
    },
  };
}

export function testQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

/** Renders `ui` inside a fresh query client. */
export function renderWithClient(
  ui: ReactElement,
  queryClient = testQueryClient(),
) {
  return {
    queryClient,
    ...render(
      <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
    ),
  };
}

/** Renders the real app (every route, the real query client) at `path`. */
export function renderApp(path: string) {
  let router: ReturnType<typeof makeRouter>;
  const { queryClient, freshness } = createQueryClient({
    onSignedOut: () => void router.invalidate(),
  });
  function makeRouter() {
    return createRouter({
      routeTree,
      context: { queryClient, freshness },
      history: createMemoryHistory({ initialEntries: [path] }),
    });
  }
  router = makeRouter();
  const result = render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { ...result, router, queryClient, freshness };
}
