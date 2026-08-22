import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { ApiUserDtoUserResponse } from "@/api/schemas";
import { useTheme } from "@/hooks/use-theme";
import { authFetch } from "@/lib/auth";
import type { ScoreFormat } from "@/lib/score-format";

export const Route = createFileRoute("/_authenticated/settings/")({
  component: SettingsPage,
});

const DEFAULT_SCORE_FORMAT: ScoreFormat = "10-point";

/** `settings` is free-form JSON on the API; read the one key we own. */
function readScoreFormat(settings: unknown): ScoreFormat {
  if (settings && typeof settings === "object") {
    const value = (settings as Record<string, unknown>).score_format;
    if (value === "5-star" || value === "10-point" || value === "100-point") {
      return value;
    }
  }
  return DEFAULT_SCORE_FORMAT;
}

function SettingsPage() {
  const { theme, setTheme } = useTheme();
  const queryClient = useQueryClient();

  const me = useQuery({
    queryKey: ["me"],
    queryFn: ({ signal }) =>
      authFetch<ApiUserDtoUserResponse>({
        url: "/api/v1/users/me",
        method: "GET",
        signal,
      }),
  });

  // Preserve any other keys the API stores alongside ours.
  const saveScoreFormat = useMutation({
    mutationFn: (format: ScoreFormat) => {
      const existing =
        me.data?.settings && typeof me.data.settings === "object"
          ? (me.data.settings as Record<string, unknown>)
          : {};
      return authFetch<ApiUserDtoUserResponse>({
        url: "/api/v1/users/me/settings",
        method: "PATCH",
        data: { settings: { ...existing, score_format: format } },
      });
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(["me"], updated);
    },
  });

  const scoreFormat = readScoreFormat(me.data?.settings);

  return (
    <div data-testid="settings-page" className="settings-page">
      {/* User settings page */}
      <section>
        <h2>Theme</h2>
        <p>Current: {theme}</p>
        <button type="button" onClick={() => setTheme("dark" as const)}>
          Dark
        </button>
        <button type="button" onClick={() => setTheme("light" as const)}>
          Light
        </button>
      </section>
      <section>
        <h2>Score Format</h2>
        <select
          value={scoreFormat}
          disabled={me.isLoading || saveScoreFormat.isPending}
          onChange={(e) =>
            saveScoreFormat.mutate(e.target.value as ScoreFormat)
          }
        >
          <option value="5-star">5 Star</option>
          <option value="10-point">10 Point</option>
          <option value="100-point">100 Point</option>
        </select>
        {saveScoreFormat.isError ? (
          <p role="alert">Couldn't save that preference. Try again.</p>
        ) : null}
      </section>
    </div>
  );
}
