import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { useAuth } from "@/hooks/use-auth";

export const Route = createFileRoute("/auth/callback")({
  validateSearch: (search: Record<string, unknown>) => ({
    code: (search.code as string | undefined) ?? "",
    state: (search.state as string | undefined) ?? "",
  }),
  component: AuthCallback,
});

function AuthCallback() {
  const { code, state } = Route.useSearch();
  const { onTokensReceived } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    if (!code) return;

    // Exchange code for tokens via the API
    fetch("/api/v1/auth/callback", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code, state }),
    })
      .then((res) => res.json())
      .then((data: { access_token: string; refresh_token: string }) => {
        onTokensReceived(data.access_token, data.refresh_token);
        navigate({ to: "/" });
      })
      .catch(() => {
        navigate({ to: "/login" });
      });
  }, [code, state, onTokensReceived, navigate]);

  return (
    <div data-testid="auth-callback" className="auth-callback">
      {/* OIDC callback handler — exchanges code for tokens then redirects */}
      <p>Completing sign-in…</p>
    </div>
  );
}
