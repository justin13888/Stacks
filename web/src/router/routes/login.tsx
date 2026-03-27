import { createFileRoute } from "@tanstack/react-router";
import LoginCard from "@/components/auth/login-card";
import { useAuth } from "@/hooks/use-auth";

export const Route = createFileRoute("/login")({
  component: LoginPage,
});

function LoginPage() {
  const { login } = useAuth();

  return (
    <div data-testid="login-page" className="login-page">
      {/* Login page — full-screen centered card */}
      <LoginCard onLogin={login} />
    </div>
  );
}
