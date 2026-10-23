// Light, dark or system theme, persisted to the account (`PATCH /me`).
//
// The account's `preferences.theme` is the source of truth, so the choice
// follows the user to every browser. The last applied theme is also kept in
// localStorage purely as a paint hint, so a reload does not flash the wrong
// palette while the session loads; it never overrides the account.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useSyncExternalStore } from "react";
import { getGetMeQueryKey, updateMe } from "@/api/account/account";
import type { CurrentSession, Theme } from "@/api/schemas";
import { toast } from "@/components/ui/toast";
import { sessionQueryOptions } from "./use-auth";

export type ResolvedTheme = "light" | "dark";

const HINT_KEY = "stacks.theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";

function darkQuery(): MediaQueryList | null {
  if (typeof window === "undefined" || !window.matchMedia) return null;
  return window.matchMedia(DARK_QUERY);
}

function systemPrefersDark(): boolean {
  return darkQuery()?.matches ?? false;
}

function subscribeToSystem(onChange: () => void): () => void {
  const query = darkQuery();
  query?.addEventListener("change", onChange);
  return () => query?.removeEventListener("change", onChange);
}

export function resolveTheme(
  theme: Theme,
  prefersDark: boolean,
): ResolvedTheme {
  if (theme === "system") return prefersDark ? "dark" : "light";
  return theme;
}

/** Writes the palette onto <html>, where the tokens read it. */
export function applyTheme(resolved: ResolvedTheme): void {
  document.documentElement.dataset.theme = resolved;
}

function isTheme(value: unknown): value is Theme {
  return value === "system" || value === "light" || value === "dark";
}

/** The theme to paint before the session has loaded. */
export function readThemeHint(): Theme {
  try {
    const stored = window.localStorage.getItem(HINT_KEY);
    return isTheme(stored) ? stored : "system";
  } catch {
    return "system";
  }
}

function writeThemeHint(theme: Theme): void {
  try {
    window.localStorage.setItem(HINT_KEY, theme);
  } catch {
    // Storage can be unavailable (privacy modes); the hint is optional.
  }
}

/** Paints the hinted theme; call once before the first render. */
export function applyInitialTheme(): void {
  applyTheme(resolveTheme(readThemeHint(), systemPrefersDark()));
}

export function useTheme() {
  const queryClient = useQueryClient();
  const { data: session } = useQuery(sessionQueryOptions);
  const theme: Theme = session?.user.preferences.theme ?? readThemeHint();
  const prefersDark = useSyncExternalStore(
    subscribeToSystem,
    systemPrefersDark,
    () => false,
  );
  const resolvedTheme = resolveTheme(theme, prefersDark);

  useEffect(() => {
    applyTheme(resolvedTheme);
    writeThemeHint(theme);
  }, [theme, resolvedTheme]);

  const mutation = useMutation({
    mutationFn: (next: Theme) => updateMe({ preferences: { theme: next } }),
    onMutate: async (next) => {
      const key = sessionQueryOptions.queryKey;
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData(key);
      if (previous) {
        const optimistic: CurrentSession = {
          ...previous,
          user: {
            ...previous.user,
            preferences: { ...previous.user.preferences, theme: next },
          },
        };
        queryClient.setQueryData(key, optimistic);
      }
      return { previous };
    },
    onError: (_error, _next, context) => {
      if (context?.previous) {
        queryClient.setQueryData(
          sessionQueryOptions.queryKey,
          context.previous,
        );
      }
      toast({ tone: "error", title: "Couldn't save your theme" });
    },
    onSuccess: (user) => {
      queryClient.setQueryData(getGetMeQueryKey(), user);
      queryClient.setQueryData(sessionQueryOptions.queryKey, (current) =>
        current ? { ...current, user } : current,
      );
    },
  });

  return {
    theme,
    resolvedTheme,
    setTheme: (next: Theme) => mutation.mutate(next),
  };
}
