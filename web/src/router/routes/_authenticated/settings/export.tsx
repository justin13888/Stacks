import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { getAccessToken } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/settings/export")({
  component: ExportPage,
});

const EXPORT_FORMATS = ["JSON", "CSV"] as const;
type ExportFormat = (typeof EXPORT_FORMATS)[number];

function ExportPage() {
  const [format, setFormat] = useState<ExportFormat>("JSON");
  const [busy, setBusy] = useState(false);

  const handleExport = async () => {
    setBusy(true);
    try {
      const token = getAccessToken();
      const headers: Record<string, string> = {};
      if (token) headers.Authorization = `Bearer ${token}`;
      const res = await fetch(
        `/api/v1/users/me/export?format=${format.toLowerCase()}`,
        { headers },
      );
      if (!res.ok) throw new Error(`Export failed: ${res.status}`);
      const blob = await res.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `stacks-export.${format.toLowerCase()}`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div data-testid="export-page" className="export-page">
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
      <button type="button" onClick={handleExport} disabled={busy}>
        {busy ? "Preparing…" : "Export"}
      </button>
    </div>
  );
}
