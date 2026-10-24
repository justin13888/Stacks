import { Link } from "@tanstack/react-router";
import { Download, FileJson, FileSpreadsheet } from "lucide-react";
import { useState } from "react";
import { getExportBackupUrl, getExportCsvUrl } from "@/api/exports/exports";
import { PageHeader } from "@/components/layout/page";
import { cn } from "@/lib/cn";
import { SettingsSection } from "./section";
import { SettingsNav } from "./settings-nav";

export const RESTORE_DOCS_URL =
  "https://github.com/justin13888/Stacks/blob/master/docs/backup-format.md";

const downloadClass = cn(
  "inline-flex h-10 items-center justify-center gap-2 self-start rounded-md bg-accent px-4 text-sm font-medium",
  "text-fg-on-accent shadow-raised hover:bg-accent-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
);

/**
 * `/settings/data`: the lossless backup and the CSV. Each is a plain link to
 * the export operation's URL: the server names the file
 * (`Content-Disposition`) and the browser streams it to disk, with the
 * session cookie, so a large library never sits in memory.
 */
export function DataPage() {
  const [includeSecrets, setIncludeSecrets] = useState(false);
  return (
    <>
      <PageHeader
        title="Backup & export"
        description="Take your data with you, or keep a copy somewhere safe."
      />
      <SettingsNav />
      <div className="flex flex-col gap-6">
        <SettingsSection
          title="Stacks backup"
          description="Everything your account holds, in one JSON file: every entry, rewatch, play, score, note, list, tag and setting, with their dates. Restoring it gives the same data back, so this is the copy to keep."
        >
          <FileJson aria-hidden className="size-8 text-accent" />
          <label className="flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              checked={includeSecrets}
              onChange={(event) => setIncludeSecrets(event.target.checked)}
              className="mt-0.5 size-4 accent-(--color-accent)"
            />
            <span>
              Include notification channel settings. They hold webhook URLs,
              signing secrets and header values, so keep a backup with them as
              safe as a password.
            </span>
          </label>
          <a
            href={getExportBackupUrl(
              includeSecrets ? { include_secrets: true } : undefined,
            )}
            download
            className={downloadClass}
          >
            <Download aria-hidden className="size-4" />
            Download backup
          </a>
          <p className="text-sm text-fg-muted">
            Restore a backup from{" "}
            <Link to="/settings/import" className="text-accent hover:underline">
              Import
            </Link>
            ; a dry run shows what it would change first.{" "}
            <a
              href={RESTORE_DOCS_URL}
              target="_blank"
              rel="noreferrer"
              className="text-accent hover:underline"
            >
              How restoring works
            </a>
            .
          </p>
        </SettingsSection>
        <SettingsSection
          title="CSV"
          description="Your library as a spreadsheet, one row per title. It is for reading, not restoring: it counts plays and rewatches rather than keeping each one, leaves out lists, the trash and settings, and can't be imported, so it can't bring an account back."
        >
          <FileSpreadsheet aria-hidden className="size-8 text-accent" />
          <a href={getExportCsvUrl()} download className={downloadClass}>
            <Download aria-hidden className="size-4" />
            Download CSV
          </a>
        </SettingsSection>
      </div>
    </>
  );
}
