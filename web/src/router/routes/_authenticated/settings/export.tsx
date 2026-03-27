import { createLazyFileRoute } from "@tanstack/react-router";
import { useState } from "react";

export const Route = createLazyFileRoute("/_authenticated/settings/export")({
  component: ExportPage,
});

const EXPORT_FORMATS = ["JSON", "CSV", "AniList", "MyAnimeList"] as const;
type ExportFormat = (typeof EXPORT_FORMATS)[number];

function ExportPage() {
  const [format, setFormat] = useState<ExportFormat>("JSON");

  return (
    <div data-testid="export-page" className="export-page">
      {/* Export page — format selection placeholder */}
      <h1>Export Library</h1>
      <label htmlFor="export-format">Format</label>
      <select
        id="export-format"
        value={format}
        onChange={(e) => setFormat(e.target.value as ExportFormat)}
      >
        {EXPORT_FORMATS.map((f) => (
          <option key={f} value={f}>
            {f}
          </option>
        ))}
      </select>
      <button type="button">Export</button>
    </div>
  );
}
