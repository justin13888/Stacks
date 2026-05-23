import { useQuery } from "@tanstack/react-query";
import type { ApiImportExportDtoImportStatusResponse } from "@/api/schemas";
import { authFetch } from "@/lib/auth";

interface ImportProgressProps {
  jobId: string;
}

/**
 * Polls the import-job status endpoint every 2s while the job is in
 * `pending` or `running` state.
 */
export default function ImportProgress({ jobId }: ImportProgressProps) {
  const status = useQuery<ApiImportExportDtoImportStatusResponse>({
    queryKey: ["import-job", jobId],
    queryFn: ({ signal }) =>
      authFetch<ApiImportExportDtoImportStatusResponse>({
        url: `/api/v1/users/me/import/${jobId}`,
        method: "GET",
        signal,
      }),
    refetchInterval: (query) => {
      const data = query.state.data;
      if (!data) return 2000;
      if (data.status === "completed" || data.status === "failed") return false;
      return 2000;
    },
  });

  if (status.isLoading || !status.data) {
    return (
      <div data-testid="import-progress" className="import-progress">
        <p>Starting import…</p>
      </div>
    );
  }

  if (status.isError) {
    return (
      <div data-testid="import-progress" className="import-progress">
        <p>Failed to fetch import status.</p>
      </div>
    );
  }

  const { status: jobStatus, attempts, error } = status.data;
  const isFinal = jobStatus === "completed" || jobStatus === "failed";

  return (
    <div data-testid="import-progress" className="import-progress">
      <p className="import-progress-status">Status: {jobStatus}</p>
      {!isFinal && (
        <progress
          className="import-progress-bar"
          aria-label="Import progress"
        />
      )}
      <p className="import-progress-attempts">Attempts: {attempts}</p>
      {error && <p className="import-progress-error">Error: {error}</p>}
    </div>
  );
}
