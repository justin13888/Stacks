interface LoginCardProps {
  onLogin: () => void;
}

export default function LoginCard({ onLogin }: LoginCardProps) {
  return (
    <div data-testid="login-card" className="login-card">
      {/* Login card stub — OIDC sign-in */}
      <h1 className="login-card-title">Stacks</h1>
      <p className="login-card-subtitle">Your personal media tracker.</p>
      <button type="button" className="login-card-btn" onClick={onLogin}>
        Sign in with OIDC
      </button>
    </div>
  );
}
