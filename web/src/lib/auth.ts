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

/**
 * Two call styles are supported:
 *   1. orval mutator form: `authFetch({ url, method, data?, signal? }, options?)`
 *   2. fetch-like form:    `authFetch(url, init?)`
 *
 * Style 1 matches orval's mutator contract — orval always passes an object
 * as the first argument. Style 2 keeps the older `authFetch(url, init)` call
 * sites working (search, library batch, export) without a refactor.
 */
export interface AuthFetchConfig {
  url: string;
  method: string;
  params?: Record<string, unknown>;
  data?: unknown;
  headers?: Record<string, string>;
  signal?: AbortSignal;
}

export function authFetch<T>(
  config: AuthFetchConfig,
  options?: RequestInit,
): Promise<T>;
export function authFetch<T>(url: string, options?: RequestInit): Promise<T>;
export async function authFetch<T>(
  configOrUrl: AuthFetchConfig | string,
  options: RequestInit = {},
): Promise<T> {
  let url: string;
  let init: RequestInit;

  if (typeof configOrUrl === "string") {
    url = configOrUrl;
    init = options;
  } else {
    url = configOrUrl.url;
    if (configOrUrl.params) {
      const qs = new URLSearchParams();
      for (const [k, v] of Object.entries(configOrUrl.params)) {
        if (v != null) qs.set(k, String(v));
      }
      const sep = url.includes("?") ? "&" : "?";
      const query = qs.toString();
      if (query) url = `${url}${sep}${query}`;
    }
    init = {
      method: configOrUrl.method,
      headers: configOrUrl.headers,
      signal: configOrUrl.signal,
      ...options,
    };
    if (configOrUrl.data !== undefined) {
      init.body = JSON.stringify(configOrUrl.data);
      init.headers = {
        "Content-Type": "application/json",
        ...(init.headers as Record<string, string> | undefined),
      };
    }
  }

  const headers = new Headers(init.headers);
  if (accessToken) {
    headers.set("Authorization", `Bearer ${accessToken}`);
  }

  const response = await fetch(url, { ...init, headers });

  if (response.status === 401 && refreshToken) {
    const refreshed = await attemptTokenRefresh();
    if (refreshed) {
      headers.set("Authorization", `Bearer ${accessToken}`);
      const retried = await fetch(url, { ...init, headers });
      if (!retried.ok) {
        throw new ApiError(retried.status, await safeJson(retried));
      }
      return safeJson(retried) as Promise<T>;
    }
    clearTokens();
    throw new ApiError(401, { error: { code: "UNAUTHORIZED" } });
  }

  if (!response.ok) {
    throw new ApiError(response.status, await safeJson(response));
  }

  return safeJson(response) as Promise<T>;
}

async function safeJson(res: Response): Promise<unknown> {
  const text = await res.text();
  if (!text) return undefined;
  try {
    return JSON.parse(text);
  } catch {
    return text;
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
