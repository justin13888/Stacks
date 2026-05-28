/**
 * Auth token storage and HTTP client for orval-generated API hooks.
 * Tokens are stored in memory only (no localStorage) to prevent XSS access.
 */

let accessToken: string | null = null;
let refreshToken: string | null = null;

export function setTokens(access: string, refresh: string): void {
  accessToken = access;
  refreshToken = refresh;
}

export function clearTokens(): void {
  accessToken = null;
  refreshToken = null;
}

export function getAccessToken(): string | null {
  return accessToken;
}

export function getRefreshToken(): string | null {
  return refreshToken;
}

export function isAuthenticated(): boolean {
  return accessToken !== null;
}

/** Arguments expected by orval's mutator convention. */
export interface AuthFetchArgs {
  url: string;
  method: string;
  signal?: AbortSignal;
  data?: unknown;
  headers?: Record<string, string>;
  params?: Record<string, string | number | boolean | null | undefined>;
  responseType?: "json" | "blob" | "text";
}

/** Per-call overrides (orval's second mutator argument). */
export type AuthFetchOptions = Omit<RequestInit, "method" | "signal" | "body">;

/**
 * Custom fetch mutator used by orval-generated hooks.
 * Injects the Authorization header and handles 401 → token refresh.
 *
 * Orval calls this as `authFetch({ url, method, signal, data?, headers? }, options?)`.
 */
export async function authFetch<T>(
  args: AuthFetchArgs,
  options?: AuthFetchOptions,
): Promise<T> {
  const fullUrl = buildUrl(args.url, args.params);
  const init = buildRequestInit(args, options);

  const response = await fetch(fullUrl, init);

  if (response.status === 401 && refreshToken) {
    const refreshed = await attemptTokenRefresh();
    if (refreshed) {
      // Rebuild init so the refreshed Authorization header is picked up
      const retryInit = buildRequestInit(args, options);
      const retried = await fetch(fullUrl, retryInit);
      if (!retried.ok) {
        throw new ApiError(retried.status, await safeParseJson(retried));
      }
      return parseResponse<T>(retried, args.responseType);
    }
    // Refresh failed — clear tokens so auth guard redirects to login
    clearTokens();
    throw new ApiError(401, { error: { code: "UNAUTHORIZED" } });
  }

  if (!response.ok) {
    throw new ApiError(response.status, await safeParseJson(response));
  }

  return parseResponse<T>(response, args.responseType);
}

function buildUrl(
  url: string,
  params?: Record<string, string | number | boolean | null | undefined>,
): string {
  if (!params) return url;
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === null || value === undefined) continue;
    search.set(key, String(value));
  }
  const query = search.toString();
  if (!query) return url;
  return url.includes("?") ? `${url}&${query}` : `${url}?${query}`;
}

function buildRequestInit(
  args: AuthFetchArgs,
  options?: AuthFetchOptions,
): RequestInit {
  const headers = new Headers(options?.headers);
  // Per-call headers from orval take precedence over the defaults
  if (args.headers) {
    for (const [k, v] of Object.entries(args.headers)) headers.set(k, v);
  }
  if (accessToken) {
    headers.set("Authorization", `Bearer ${accessToken}`);
  }

  let body: BodyInit | undefined;
  if (args.data !== undefined && args.data !== null) {
    if (typeof FormData !== "undefined" && args.data instanceof FormData) {
      body = args.data;
    } else if (typeof args.data === "string") {
      body = args.data;
      if (!headers.has("Content-Type")) {
        headers.set("Content-Type", "application/json");
      }
    } else {
      body = JSON.stringify(args.data);
      if (!headers.has("Content-Type")) {
        headers.set("Content-Type", "application/json");
      }
    }
  }

  return {
    ...options,
    method: args.method,
    signal: args.signal,
    headers,
    body,
  };
}

async function parseResponse<T>(
  response: Response,
  responseType?: "json" | "blob" | "text",
): Promise<T> {
  // 204 No Content / empty bodies → return undefined as T
  if (response.status === 204) return undefined as T;
  const contentLength = response.headers.get("content-length");
  if (contentLength === "0") return undefined as T;

  if (responseType === "blob") return (await response.blob()) as T;
  if (responseType === "text") return (await response.text()) as T;

  const contentType = response.headers.get("content-type") ?? "";
  if (contentType.includes("application/json")) {
    return (await response.json()) as T;
  }
  // Best-effort fallback — return text for non-JSON success bodies
  const text = await response.text();
  if (!text) return undefined as T;
  try {
    return JSON.parse(text) as T;
  } catch {
    return text as unknown as T;
  }
}

async function safeParseJson(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return null;
  }
}

async function attemptTokenRefresh(): Promise<boolean> {
  if (!refreshToken) return false;
  try {
    const response = await fetch("/api/v1/auth/refresh", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    if (!response.ok) return false;
    const data = (await response.json()) as {
      access_token: string;
      refresh_token: string;
    };
    setTokens(data.access_token, data.refresh_token);
    return true;
  } catch {
    return false;
  }
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly body: unknown,
  ) {
    super(`API error: ${status}`);
  }
}
