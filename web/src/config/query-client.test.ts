// The global failure policy: which failures are retried and after how long,
// which ones raise a toast, and that a 401 anywhere signs the user out.

import { MutationObserver, type QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { toast } from "@/components/ui/toast";
import { ApiProblem } from "@/lib/api-client";
import { createQueryClient } from "./query-client";

vi.mock("@/components/ui/toast", () => ({ toast: vi.fn() }));

function problem(status: number, headers: Record<string, string> = {}) {
  return new ApiProblem(
    status,
    { type: "about:blank", status },
    new Headers(headers),
  );
}

const NETWORK = new TypeError("Failed to fetch");

function setup() {
  const onSignedOut = vi.fn();
  const { queryClient } = createQueryClient({ onSignedOut });
  return { queryClient, onSignedOut };
}

function policy(queryClient: QueryClient) {
  const { retry, retryDelay } = queryClient.getDefaultOptions().queries ?? {};
  return {
    retry: retry as (failureCount: number, error: unknown) => boolean,
    retryDelay: retryDelay as (attempt: number, error: unknown) => number,
  };
}

/** Runs a query to completion, timers included; resolves to how many times it ran. */
async function attempts(
  queryClient: QueryClient,
  error: unknown,
): Promise<number> {
  const queryFn = vi.fn(async () => {
    throw error;
  });
  const done = queryClient
    .fetchQuery({ queryKey: ["attempts"], queryFn })
    .catch(() => {});
  await vi.runAllTimersAsync();
  await done;
  return queryFn.mock.calls.length;
}

describe("query retries", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  test("upstream, rate-limit and network failures are retried exactly once", async () => {
    for (const error of [problem(429), problem(502), problem(503), NETWORK]) {
      const { queryClient } = setup();
      expect(await attempts(queryClient, error)).toBe(2);
    }
  });

  test("client errors and other server errors are never retried", async () => {
    for (const status of [400, 401, 403, 404, 409, 422, 500]) {
      const { queryClient } = setup();
      expect(await attempts(queryClient, problem(status))).toBe(1);
    }
  });

  test("the retry waits for Retry-After, capped at 10 s, else 1 s", () => {
    const { retryDelay, retry } = policy(setup().queryClient);
    expect(retryDelay(0, problem(503, { "retry-after": "3" }))).toBe(3_000);
    expect(retryDelay(0, problem(429, { "retry-after": "120" }))).toBe(10_000);
    expect(retryDelay(0, problem(502))).toBe(1_000);
    expect(retryDelay(0, NETWORK)).toBe(1_000);
    expect(retry(1, problem(503))).toBe(false);
  });

  test("mutations are never retried", async () => {
    const { queryClient } = setup();
    const mutationFn = vi.fn(async () => {
      throw problem(503);
    });
    const done = new MutationObserver(queryClient, { mutationFn })
      .mutate()
      .catch(() => {});
    await vi.runAllTimersAsync();
    await done;
    expect(mutationFn).toHaveBeenCalledTimes(1);
  });
});

describe("failure reporting", () => {
  test("a failed background refetch raises a toast; a failed first load does not", async () => {
    const { queryClient } = setup();
    await queryClient
      .fetchQuery({
        queryKey: ["first"],
        queryFn: () => Promise.reject(problem(404)),
      })
      .catch(() => {});
    expect(toast).not.toHaveBeenCalled();

    queryClient.setQueryData(["shown"], "cached");
    await queryClient
      .fetchQuery({
        queryKey: ["shown"],
        queryFn: () => Promise.reject(problem(404)),
        staleTime: 0,
      })
      .catch(() => {});
    expect(toast).toHaveBeenCalledTimes(1);
    expect(toast).toHaveBeenCalledWith(
      expect.objectContaining({ tone: "error" }),
    );
  });

  test("a failed mutation raises a toast unless it handles its own errors", async () => {
    const { queryClient } = setup();
    const mutationFn = () => Promise.reject(problem(409));
    const run = (options: object) =>
      new MutationObserver(queryClient, { mutationFn, ...options })
        .mutate()
        .catch(() => {});

    await run({ onError: () => {} });
    await run({ meta: { handlesErrors: true } });
    expect(toast).not.toHaveBeenCalled();
    await run({});
    expect(toast).toHaveBeenCalledTimes(1);
  });

  test("a 401 from a query or a mutation signs out without a toast", async () => {
    const { queryClient, onSignedOut } = setup();
    queryClient.setQueryData(["shown"], "cached");
    await queryClient
      .fetchQuery({
        queryKey: ["shown"],
        queryFn: () => Promise.reject(problem(401)),
        staleTime: 0,
      })
      .catch(() => {});
    await new MutationObserver(queryClient, {
      mutationFn: () => Promise.reject(problem(401)),
    })
      .mutate()
      .catch(() => {});
    expect(onSignedOut).toHaveBeenCalledTimes(2);
    expect(toast).not.toHaveBeenCalled();
  });
});
