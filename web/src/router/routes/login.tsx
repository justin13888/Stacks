import { createFileRoute, redirect } from "@tanstack/react-router";
import { LogIn } from "lucide-react";
import { Logo } from "@/components/layout/logo";
import { Button } from "@/components/ui/button";
import { ErrorState } from "@/components/ui/error-state";
import { loadSession, useSignIn } from "@/hooks/use-auth";

export const Route = createFileRoute("/login")({
  // Already signed in: there is nothing to do here.
  beforeLoad: async ({ context }) => {
    const session = await loadSession(context.queryClient).catch(() => null);
    if (session) throw redirect({ to: "/" });
  },
  component: LoginPage,
});

export function LoginPage() {
  const signIn = useSignIn();
  return (
    <main
      data-testid="login-page"
      className="relative flex min-h-dvh items-center justify-center overflow-hidden bg-canvas px-4"
    >
      <div
        aria-hidden
        className="pointer-events-none absolute -top-40 left-1/2 size-[42rem] -translate-x-1/2 rounded-full bg-accent/20 blur-3xl"
      />
      <section className="relative flex w-full max-w-sm flex-col items-center gap-6 rounded-xl border border-line bg-surface p-8 text-center shadow-overlay">
        <Logo />
        <div className="flex flex-col gap-2">
          <h1 className="font-display text-2xl font-bold tracking-tight">
            Everything you watch, in one place
          </h1>
          <p className="text-sm text-fg-muted">
            Movies, shows, anime and dramas: track them, rate them, never lose
            your place.
          </p>
        </div>
        <Button
          size="lg"
          className="w-full"
          loading={signIn.isPending || signIn.isSuccess}
          onClick={() => signIn.mutate()}
        >
          {!signIn.isPending && !signIn.isSuccess && (
            <LogIn aria-hidden className="size-5" />
          )}
          Sign in
        </Button>
        {signIn.isError && <ErrorState error={signIn.error} className="py-0" />}
      </section>
    </main>
  );
}
