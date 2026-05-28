import { useMutation } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import type { ApiImportExportDtoImportStatusResponse } from "@/api/schemas";
import ImportForm from "@/components/import/import-form";
import ImportProgress from "@/components/import/import-progress";
import { authFetch } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/settings/import")({
  component: ImportPage,
});

interface StartImportPayload {
  source: string;
  conflict_strategy: string;
  data_base64: string;
}

async function fileToBase64(file: File): Promise<string> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  // Build a binary string in 8KB chunks (avoids "max call stack" on big files)
  const CHUNK = 0x2000;
  let binary = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    const slice = bytes.subarray(i, i + CHUNK);
    binary += String.fromCharCode(...slice);
  }
  return btoa(binary);
}

function ImportPage() {
  const [jobId, setJobId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const startImport = useMutation({
    mutationFn: (payload: StartImportPayload) =>
      authFetch<ApiImportExportDtoImportStatusResponse>({
        url: "/api/v1/users/me/import",
        method: "POST",
        data: payload,
      }),
    onSuccess: (response) => {
      setJobId(response.job_id);
    },
    onError: (err: unknown) => {
      setError(err instanceof Error ? err.message : "Failed to start import");
    },
  });

  const handleSubmit = async (file: File, source: string, conflict: string) => {
    setError(null);
    setJobId(null);
    try {
      const base64 = await fileToBase64(file);
      startImport.mutate({
        source,
        conflict_strategy: conflict,
        data_base64: base64,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to read file");
    }
  };

  return (
    <div data-testid="import-page" className="import-page">
      <h1>Import Library</h1>
      <ImportForm onSubmit={handleSubmit} />
      {startImport.isPending && <p>Submitting import…</p>}
      {error && <p className="import-error">{error}</p>}
      {jobId && <ImportProgress jobId={jobId} />}
    </div>
  );
}
