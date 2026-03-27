import { useCallback, useEffect, useState } from "react";
import {
  clearTokens,
  getAccessToken,
  isAuthenticated,
  setTokens,
} from "@/lib/auth";

interface AuthState {
  isLoggedIn: boolean;
  userId: string | null;
}

/** Hook for reading auth state and triggering OIDC login / logout. */
export function useAuth() {
  const [state, setState] = useState<AuthState>({
    isLoggedIn: isAuthenticated(),
    userId: null,
  });

  useEffect(() => {
    setState({ isLoggedIn: isAuthenticated(), userId: null });
  }, []);

  /** Redirect to the OIDC provider's authorization URL. */
  const login = useCallback(async () => {
    const res = await fetch("/api/v1/auth/login");
    const data = (await res.json()) as { auth_url: string };
    window.location.href = data.auth_url;
  }, []);

  /** Clear tokens and redirect to login. */
  const logout = useCallback(async () => {
    const token = getAccessToken();
    if (token) {
      await fetch("/api/v1/auth/logout", {
        method: "POST",
        headers: { Authorization: `Bearer ${token}` },
      }).catch(() => undefined);
    }
    clearTokens();
    setState({ isLoggedIn: false, userId: null });
    window.location.href = "/login";
  }, []);

  /** Called from the OIDC callback route after code exchange. */
  const onTokensReceived = useCallback((access: string, refresh: string) => {
    setTokens(access, refresh);
    setState({ isLoggedIn: true, userId: null });
  }, []);

  return { ...state, login, logout, onTokensReceived };
}
