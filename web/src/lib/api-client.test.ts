import { describe, expect, test } from "vitest";
import { json, mockApi, problem } from "@/test/utils";
import { ApiProblem, apiFetch } from "./api-client";

describe("apiFetch", () => {
  test("sends the session cookie and decodes JSON", async () => {
    const fetchMock = mockApi(() => json({ ok: true }));
    await expect(apiFetch("/api/v1/me", { method: "GET" })).resolves.toEqual({
      ok: true,
    });
    const [, init] = fetchMock.mock.calls[0] ?? [];
    expect(init?.credentials).toBe("include");
    expect(new Headers(init?.headers).get("accept")).toContain(
      "application/problem+json",
    );
  });

  test("a problem+json failure is thrown as a typed ApiProblem", async () => {
    mockApi(() =>
      problem(503, "upstream-unavailable", { "Retry-After": "30" }),
    );
    const error = await apiFetch("/api/v1/search?q=x").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiProblem);
    const p = error as ApiProblem;
    expect(p.status).toBe(503);
    expect(p.kind).toBe("upstream-unavailable");
    expect(p.retryAfterSeconds).toBe(30);
    expect(p.retryable).toBe(true);
  });

  test("a failure without a problem body still becomes an ApiProblem", async () => {
    mockApi(
      () =>
        new Response("<html>Bad gateway</html>", {
          status: 502,
          statusText: "Bad Gateway",
        }),
    );
    const error = (await apiFetch("/api/v1/me").catch(
      (e: unknown) => e,
    )) as ApiProblem;
    expect(error).toBeInstanceOf(ApiProblem);
    expect(error.problem).toEqual({
      type: "about:blank",
      status: 502,
      title: "Bad Gateway",
    });
    expect(error.kind).toBeUndefined();
  });

  test("a 401 is not retryable and has no kind", async () => {
    mockApi(() => problem(401));
    const error = (await apiFetch("/api/v1/me").catch(
      (e: unknown) => e,
    )) as ApiProblem;
    expect(error.status).toBe(401);
    expect(error.retryable).toBe(false);
  });

  test("an empty success is undefined; a binary one is a Blob", async () => {
    mockApi(() => new Response(null, { status: 204 }));
    await expect(
      apiFetch("/api/v1/sessions/current", { method: "DELETE" }),
    ).resolves.toBeUndefined();

    mockApi(
      () => new Response("a,b\n", { headers: { "content-type": "text/csv" } }),
    );
    const csv = await apiFetch<Blob>("/api/v1/exports/csv");
    // The runtime's own Blob (jsdom and undici each have one), so no instanceof.
    expect(csv.type).toBe("text/csv");
    expect(await csv.text()).toBe("a,b\n");
  });

  test("a multipart body lets fetch write its own boundary", async () => {
    const fetchMock = mockApi(() => json({}, 201));
    const form = new FormData();
    form.append("source", "mal_xml");
    await apiFetch("/api/v1/imports", {
      method: "POST",
      headers: { "Content-Type": "multipart/form-data" },
      body: form,
    });
    const [, init] = fetchMock.mock.calls[0] ?? [];
    expect(new Headers(init?.headers).has("content-type")).toBe(false);
  });
});
