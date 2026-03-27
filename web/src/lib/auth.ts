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
 * Custom fetch mutator used by orval-generated hooks.
 * Injects the Authorization header and handles 401 → token refresh.
 */
export async function authFetch<T>(
  url: string,
  options: RequestInit,
): Promise<T> {
  const headers = new Headers(options.headers);
  if (accessToken) {
    headers.set("Authorization", `Bearer ${accessToken}`);
  }

  const response = await fetch(url, { ...options, headers });

  if (response.status === 401 && refreshToken) {
    const refreshed = await attemptTokenRefresh();
    if (refreshed) {
      headers.set("Authorization", `Bearer ${accessToken}`);
      const retried = await fetch(url, { ...options, headers });
      if (!retried.ok) {
        throw new ApiError(retried.status, await retried.json());
      }
      return retried.json() as Promise<T>;
    }
    // Refresh failed — clear tokens so auth guard redirects to login
    clearTokens();
    throw new ApiError(401, { error: { code: "UNAUTHORIZED" } });
  }

  if (!response.ok) {
    throw new ApiError(response.status, await response.json());
  }

  return response.json() as Promise<T>;
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
