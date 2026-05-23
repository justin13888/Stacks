import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { getAccessToken } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/settings/export")({
  component: ExportPage,
});

const EXPORT_FORMATS = [
  { value: "json", label: "JSON" },
  { value: "csv", label: "CSV" },
] as const;
type ExportFormat = (typeof EXPORT_FORMATS)[number]["value"];

function defaultFilename(format: ExportFormat): string {
  const stamp = new Date().toISOString().slice(0, 10);
  return `stacks-export-${stamp}.${format}`;
}

/** Extract a filename from a Content-Disposition header, if present. */
function extractFilename(header: string | null): string | null {
  if (!header) return null;
  // RFC 5987: filename*=UTF-8''…
  const star = /filename\*\s*=\s*([^']+)''([^;]+)/i.exec(header);
  if (star?.[2]) {
    try {
      return decodeURIComponent(star[2]);
    } catch {
      return star[2];
    }
  }
  const quoted = /filename\s*=\s*"([^"]+)"/i.exec(header);
  if (quoted?.[1]) return quoted[1];
  const bare = /filename\s*=\s*([^;]+)/i.exec(header);
  return bare?.[1]?.trim() ?? null;
}

function ExportPage() {
  const [format, setFormat] = useState<ExportFormat>("json");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleExport = async () => {
    setError(null);
    setBusy(true);
    try {
      const token = getAccessToken();
      const response = await fetch(
        `/api/v1/users/me/export?format=${encodeURIComponent(format)}`,
        {
          method: "GET",
          headers: token ? { Authorization: `Bearer ${token}` } : undefined,
        },
      );
      if (!response.ok) {
        throw new Error(`Export failed (${response.status})`);
      }
      const blob = await response.blob();
      const filename =
        extractFilename(response.headers.get("Content-Disposition")) ??
        defaultFilename(format);

      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Export failed");
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
          <option key={f.value} value={f.value}>
            {f.label}
          </option>
        ))}
      </select>
      <button type="button" onClick={handleExport} disabled={busy}>
        {busy ? "Exporting…" : "Export"}
      </button>
      {error && <p className="export-error">{error}</p>}
    </div>
  );
}
