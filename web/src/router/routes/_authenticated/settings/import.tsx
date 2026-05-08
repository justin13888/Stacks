import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import ImportForm from "@/components/import/import-form";
import ImportProgress from "@/components/import/import-progress";
import { authFetch } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/settings/import")({
  component: ImportPage,
});

const FORMAT_TO_API: Record<string, string> = {
  AniList: "mal_json",
  MyAnimeList: "mal_xml",
  Trakt: "tmdb_json",
  Letterboxd: "tmdb_json",
};

async function fileToBase64(file: File): Promise<string> {
  const buf = await file.arrayBuffer();
  const bytes = new Uint8Array(buf);
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i] ?? 0;
    binary += String.fromCharCode(b);
  }
  return btoa(binary);
}

interface ImportStatusResponse {
  job_id: string;
  status: string;
}

function ImportPage() {
  const [jobId, setJobId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (file: File, format: string, conflict: string) => {
    setError(null);
    try {
      const data_base64 = await fileToBase64(file);
      const apiFormat = FORMAT_TO_API[format] ?? "stacks_json";
      const res = await authFetch<ImportStatusResponse>(
        "/api/v1/users/me/import",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            source: apiFormat,
            data_base64,
            conflict_strategy: conflict,
          }),
        },
      );
      setJobId(res.job_id);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Import failed");
    }
  };

  return (
    <div data-testid="import-page" className="import-page">
      <ImportForm onSubmit={handleSubmit} />
      {error && <p className="import-error">Error: {error}</p>}
      {jobId && <ImportProgress jobId={jobId} />}
    </div>
  );
}
