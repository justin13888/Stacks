import { useQueryClient } from "@tanstack/react-query";
import { Upload } from "lucide-react";
import { type FormEvent, useId, useState } from "react";
import {
  getListImportsQueryKey,
  useCreateImport,
  useRestoreBackup,
} from "@/api/imports/imports";
import type { ConflictStrategy, RestoreMode } from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { reportFailure } from "@/components/ui/error-state";
import { Select } from "@/components/ui/select";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";
import { RestoreReportView } from "./restore-report";
import {
  MODE_EFFECTS,
  MODE_OPTIONS,
  SOURCES,
  STRATEGY_OPTIONS,
  WIZARD_SOURCES,
  type WizardSource,
} from "./sources";

export const BACKUP_STEPS = ["Choose a file", "Preview", "Restore"] as const;
export const IMPORT_STEPS = ["Choose a file", "Import", "Resolve"] as const;

/** The wizard's place, for its step indicator. */
export function Steps({
  labels,
  current,
}: {
  labels: readonly string[];
  current: number;
}) {
  return (
    <ol aria-label="Steps" className="flex flex-wrap gap-x-4 gap-y-1 text-sm">
      {labels.map((label, index) => (
        <li
          key={label}
          aria-current={index === current ? "step" : undefined}
          className={cn(
            "flex items-center gap-1.5",
            index === current ? "font-medium text-fg" : "text-fg-subtle",
          )}
        >
          <span
            aria-hidden
            className={cn(
              "inline-flex size-5 items-center justify-center rounded-full text-xs",
              index <= current
                ? "bg-accent text-fg-on-accent"
                : "bg-surface-sunken",
            )}
          >
            {index + 1}
          </span>
          {label}
        </li>
      ))}
    </ol>
  );
}

/**
 * Step 1 (and, for a backup, step 2): the source and its file. Another
 * tracker's export is queued as an import, which `onStarted` opens; a
 * backup is previewed with a dry run and restored only once confirmed.
 */
export function ImportWizard({
  onStarted,
}: {
  onStarted: (importId: string) => void;
}) {
  const fileId = useId();
  const [source, setSource] = useState<WizardSource>("backup");
  const [file, setFile] = useState<File | null>(null);
  const [strategy, setStrategy] = useState<ConflictStrategy>("skip");
  const [backupStep, setBackupStep] = useState(0);

  function choose(next: WizardSource) {
    setSource(next);
    setFile(null);
    setBackupStep(0);
  }

  return (
    <div className="flex flex-col gap-4">
      {source === "backup" ? (
        <Steps labels={BACKUP_STEPS} current={backupStep} />
      ) : (
        <Steps labels={IMPORT_STEPS} current={0} />
      )}
      <fieldset className="grid gap-2 sm:grid-cols-2">
        <legend className="mb-2 text-sm font-medium text-fg">
          What are you importing?
        </legend>
        {WIZARD_SOURCES.map((value) => (
          <label
            key={value}
            className={cn(
              "flex cursor-pointer items-start gap-3 rounded-lg border p-3",
              source === value ? "border-accent bg-accent-soft" : "border-line",
            )}
          >
            <input
              type="radio"
              name="import-source"
              value={value}
              checked={source === value}
              onChange={() => choose(value)}
              className="mt-1 size-4 accent-(--color-accent)"
            />
            <span className="flex flex-col gap-0.5">
              <span className="text-sm font-medium text-fg">
                {SOURCES[value].label}
              </span>
              <span className="text-xs text-fg-muted">
                {SOURCES[value].description}
              </span>
            </span>
          </label>
        ))}
      </fieldset>
      <div className="flex flex-col gap-1.5">
        <label htmlFor={fileId} className="text-sm font-medium text-fg">
          File
        </label>
        <input
          // A new source clears the picked file.
          key={source}
          id={fileId}
          type="file"
          accept={SOURCES[source].accept}
          onChange={(event) => {
            setFile(event.target.files?.[0] ?? null);
            setBackupStep(0);
          }}
          className="text-sm file:mr-3 file:rounded-md file:border file:border-line file:bg-surface file:px-3 file:py-2 file:text-sm file:text-fg"
        />
      </div>
      {source === "backup" ? (
        <BackupRestore
          key={file ? `${file.name}:${file.size}:${file.lastModified}` : "none"}
          file={file}
          onStep={setBackupStep}
        />
      ) : (
        <StartImport
          source={source}
          file={file}
          strategy={strategy}
          onStrategyChange={setStrategy}
          onStarted={onStarted}
        />
      )}
    </div>
  );
}

function StartImport({
  source,
  file,
  strategy,
  onStrategyChange,
  onStarted,
}: {
  source: Exclude<WizardSource, "backup">;
  file: File | null;
  strategy: ConflictStrategy;
  onStrategyChange: (strategy: ConflictStrategy) => void;
  onStarted: (importId: string) => void;
}) {
  const queryClient = useQueryClient();
  const create = useCreateImport({
    mutation: {
      onSuccess: (started) => {
        void queryClient.invalidateQueries({
          queryKey: getListImportsQueryKey(),
        });
        onStarted(started.id);
      },
      onError: reportFailure,
    },
  });
  function submit(event: FormEvent) {
    event.preventDefault();
    if (file) {
      create.mutate({
        data: { source, conflict_strategy: strategy, file },
      });
    }
  }
  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <Select
        label="Titles already in your library"
        value={strategy}
        options={STRATEGY_OPTIONS}
        onValueChange={onStrategyChange}
      />
      <Button
        type="submit"
        className="self-start"
        disabled={!file}
        loading={create.isPending}
      >
        <Upload aria-hidden className="size-4" />
        Start import
      </Button>
    </form>
  );
}

/**
 * A backup restore: a dry run first (the server does the whole restore in a
 * transaction, reports it, and rolls it back), then the real one only when
 * the user confirms. Changing the mode discards the preview.
 */
function BackupRestore({
  file,
  onStep,
}: {
  file: File | null;
  onStep: (step: number) => void;
}) {
  const queryClient = useQueryClient();
  const [mode, setMode] = useState<RestoreMode>("merge");
  const [confirming, setConfirming] = useState(false);
  const preview = useRestoreBackup({
    mutation: { onSuccess: () => onStep(1), onError: reportFailure },
  });
  const restore = useRestoreBackup({
    mutation: {
      onSuccess: () => {
        setConfirming(false);
        onStep(2);
        toast({ tone: "success", title: "Backup restored" });
        // A restore can touch every part of the account.
        void queryClient.invalidateQueries();
      },
      onError: (error) => {
        setConfirming(false);
        reportFailure(error);
      },
    },
  });

  if (restore.data) {
    return (
      <section aria-label="Restore summary" className="flex flex-col gap-3">
        <h3 className="font-semibold text-fg">Restored</h3>
        <RestoreReportView report={restore.data} />
      </section>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <Select
        label="Restore mode"
        value={mode}
        options={MODE_OPTIONS}
        onValueChange={(next) => {
          setMode(next);
          preview.reset();
          onStep(0);
        }}
      />
      <Button
        variant={preview.data ? "secondary" : "primary"}
        className="self-start"
        disabled={!file}
        loading={preview.isPending}
        onClick={() =>
          file &&
          preview.mutate({ data: { file }, params: { mode, dry_run: true } })
        }
      >
        Preview restore
      </Button>
      {preview.data && (
        <section
          aria-label="Restore preview"
          className="flex flex-col gap-3 rounded-lg border border-line p-4"
        >
          <h3 className="font-semibold text-fg">
            Preview: nothing has changed yet
          </h3>
          <RestoreReportView report={preview.data} />
          <Button className="self-start" onClick={() => setConfirming(true)}>
            Restore backup
          </Button>
        </section>
      )}
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Restore this backup?"
        description={`The restore makes the changes the preview shows, in ${mode} mode. ${MODE_EFFECTS[mode]}`}
        confirmLabel="Restore"
        tone="primary"
        pending={restore.isPending}
        onConfirm={() =>
          file && restore.mutate({ data: { file }, params: { mode } })
        }
      />
    </div>
  );
}
