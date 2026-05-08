import { useEffect, useState } from "react";
import { authFetch } from "@/lib/auth";

interface ImportProgressProps {
  jobId: string;
}

interface ImportStatusResponse {
  job_id: string;
  status: string;
  progress: number | null;
  report: unknown;
}

export default function ImportProgress({ jobId }: ImportProgressProps) {
  const [jobStatus, setJobStatus] = useState<ImportStatusResponse | null>(null);

  useEffect(() => {
    let cancelled = false;

    const poll = async () => {
      try {
        const data = await authFetch<ImportStatusResponse>(
          `/api/v1/users/me/import/${jobId}`,
          { method: "GET" },
        );
        if (cancelled) return;
        setJobStatus(data);
        if (data.status === "running" || data.status === "pending") {
          setTimeout(poll, 2000);
        }
      } catch {
        if (!cancelled) {
          setJobStatus({
            job_id: jobId,
            status: "error",
            progress: null,
            report: { error: "Failed to fetch status." },
          });
        }
      }
    };

    poll();

    return () => {
      cancelled = true;
    };
  }, [jobId]);

  return (
    <div data-testid="import-progress" className="import-progress">
      {!jobStatus ? (
        <p>Starting import…</p>
      ) : (
        <>
          <p className="import-progress-status">Status: {jobStatus.status}</p>
          {jobStatus.progress != null && (
            <progress
              className="import-progress-bar"
              value={jobStatus.progress}
              max={100}
            />
          )}
          {jobStatus.report ? (
            <pre className="import-progress-report">
              {JSON.stringify(jobStatus.report, null, 2)}
            </pre>
          ) : null}
        </>
      )}
    </div>
  );
}
