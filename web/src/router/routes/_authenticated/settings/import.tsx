import { createLazyFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import ImportForm from "@/components/import/import-form";
import ImportProgress from "@/components/import/import-progress";

export const Route = createLazyFileRoute("/_authenticated/settings/import")({
  component: ImportPage,
});

function ImportPage() {
  const [jobId, setJobId] = useState<string | null>(null);

  const handleSubmit = (_file: File, _format: string, _conflict: string) => {
    // TODO: submit import job and receive jobId
    setJobId("placeholder-job-id");
  };

  return (
    <div data-testid="import-page" className="import-page">
      {/* Import page */}
      <ImportForm onSubmit={handleSubmit} />
      {jobId && <ImportProgress jobId={jobId} />}
    </div>
  );
}
