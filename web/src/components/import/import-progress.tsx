import { useEffect, useState } from "react";

interface ImportProgressProps {
  jobId: string;
}

interface ImportJobStatus {
  status: "pending" | "running" | "done" | "error";
  processed: number;
  total: number;
  message?: string;
}

export default function ImportProgress({ jobId }: ImportProgressProps) {
  const [jobStatus, setJobStatus] = useState<ImportJobStatus | null>(null);

  useEffect(() => {
    let cancelled = false;

    const poll = async () => {
      try {
        const res = await fetch(`/api/v1/import/jobs/${jobId}`);
        const data = (await res.json()) as ImportJobStatus;
        if (!cancelled) {
          setJobStatus(data);
          if (data.status === "running" || data.status === "pending") {
            setTimeout(poll, 2000);
          }
        }
      } catch {
        if (!cancelled) {
          setJobStatus({
            status: "error",
            processed: 0,
            total: 0,
            message: "Failed to fetch status.",
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
      {/* Import progress stub — polls job status */}
      {!jobStatus ? (
        <p>Starting import…</p>
      ) : (
        <>
          <p className="import-progress-status">Status: {jobStatus.status}</p>
          <progress
            className="import-progress-bar"
            value={jobStatus.processed}
            max={jobStatus.total || 1}
          />
          <p className="import-progress-count">
            {jobStatus.processed} / {jobStatus.total}
          </p>
          {jobStatus.message && (
            <p className="import-progress-message">{jobStatus.message}</p>
          )}
        </>
      )}
    </div>
  );
}
