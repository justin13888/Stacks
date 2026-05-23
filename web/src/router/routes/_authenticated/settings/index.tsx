import { createFileRoute } from "@tanstack/react-router";
import type { ScoreFormat } from "@/lib/score-format";
import { useTheme } from "@/hooks/use-theme";
import { useState } from "react";

export const Route = createFileRoute("/_authenticated/settings/")({
  component: SettingsPage,
});

function SettingsPage() {
  const { theme, setTheme } = useTheme();
  const [scoreFormat, setScoreFormat] = useState<ScoreFormat>("10-point");

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
          onChange={(e) => setScoreFormat(e.target.value as ScoreFormat)}
        >
          <option value="5-star">5 Star</option>
          <option value="10-point">10 Point</option>
          <option value="100-point">100 Point</option>
        </select>
      </section>
    </div>
  );
}
