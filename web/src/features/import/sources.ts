// The files the import wizard reads, and how an import's state reads in words.

import type {
  ConflictStrategy,
  ImportItem,
  ImportSource,
  ImportStatus,
  RestoreMode,
} from "@/api/schemas";

/** What the wizard can read: a Stacks backup, or another tracker's export. */
export type WizardSource = "backup" | ImportSource;

export interface SourceInfo {
  label: string;
  description: string;
  /** The file picker's `accept`. */
  accept: string;
}

export const SOURCES: Record<WizardSource, SourceInfo> = {
  backup: {
    label: "Stacks backup",
    description:
      "A backup taken from Backup & export. Restores everything it holds, with a preview first.",
    accept: ".json,application/json",
  },
  mal_xml: {
    label: "MyAnimeList XML",
    description:
      "The anime list from MyAnimeList's “Export my list” page. MAL downloads it as .xml.gz: unzip it first.",
    accept: ".xml,application/xml,text/xml",
  },
  mal_json: {
    label: "MyAnimeList JSON",
    description:
      "Your anime list as the MyAnimeList API returns it, saved as JSON.",
    accept: ".json,application/json",
  },
  tmdb_json: {
    label: "TMDB JSON",
    description:
      "Your TMDB ratings, watchlist and favourites, as the TMDB account API returns them.",
    accept: ".json,application/json",
  },
};

export const WIZARD_SOURCES = Object.keys(SOURCES) as WizardSource[];

export const SOURCE_LABELS: Record<ImportSource, string> = {
  mal_xml: SOURCES.mal_xml.label,
  mal_json: SOURCES.mal_json.label,
  tmdb_json: SOURCES.tmdb_json.label,
};

export const STRATEGY_OPTIONS: readonly {
  value: ConflictStrategy;
  label: string;
}[] = [
  { value: "skip", label: "Skip titles already in my library" },
  { value: "merge", label: "Merge: fill in what my library lacks" },
  { value: "overwrite", label: "Overwrite with the file's values" },
];

export const MODE_OPTIONS: readonly { value: RestoreMode; label: string }[] = [
  { value: "merge", label: "Merge: add what's missing, keep what I have" },
  { value: "skip", label: "Skip: leave existing rows untouched" },
  { value: "overwrite", label: "Overwrite: the backup's values win" },
];

export const STATUS_LABELS: Record<ImportStatus, string> = {
  pending: "Queued",
  running: "Importing",
  completed: "Finished",
  failed: "Failed",
};

/** Whether the worker is still reading the import. */
export function isActive(status: ImportStatus): boolean {
  return status === "pending" || status === "running";
}

/** Below this, a matched row is shown for checking. */
export const LOW_CONFIDENCE = 0.8;

/** Rows with no title, waiting for the user to choose one. */
export function unmatchedRows(items: readonly ImportItem[]): ImportItem[] {
  return items.filter((item) => item.outcome === "unmatched");
}

/** Rows applied to a title the matcher was unsure of. */
export function lowConfidenceRows(items: readonly ImportItem[]): ImportItem[] {
  return items.filter(
    (item) =>
      item.outcome === "applied" &&
      item.confidence != null &&
      item.confidence < LOW_CONFIDENCE,
  );
}

export function percent(confidence: number): string {
  return `${Math.round(confidence * 100)}%`;
}
