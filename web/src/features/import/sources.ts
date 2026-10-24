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

/**
 * What a restore in each mode does to what you already have, as
 * `core/src/import_export/backup/restore.rs` applies it. No mode removes a
 * row outright, and plays and history are only ever added.
 */
export const MODE_EFFECTS: Record<RestoreMode, string> = {
  merge:
    "It adds what your library lacks and keeps what you have. Nothing is deleted.",
  skip: "It adds what's missing and leaves everything you already have untouched. Nothing is deleted.",
  overwrite:
    "The backup's statuses, scores and notes replace yours, and a score or note the backup doesn't have is cleared. Entries and lists follow the backup's Trash: one it has deleted moves to your Trash (restore it from there), and one it has kept comes back out. Plays and history are only added.",
};

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

/**
 * Below this, a matched row is shown for checking. The server applies a row
 * matched by id or by hand at confidence 1, and one matched by a title search
 * only at its auto-match score or above (`AUTO_MATCH`, 0.88, in
 * `core/src/import_export/import/matching.rs`), so every applied row under 1
 * is a title-search match: the ones worth a check. One under 0.95
 * (`REVIEW_BELOW`) also carries the candidates it was chosen from.
 */
export const LOW_CONFIDENCE = 1;

/** Rows with no title, waiting for the user to choose one. */
export function unmatchedRows(items: readonly ImportItem[]): ImportItem[] {
  return items.filter((item) => item.outcome === "unmatched");
}

/** Rows set aside by hand: skipped, with no title. */
export function skippedRows(items: readonly ImportItem[]): ImportItem[] {
  return items.filter((item) => item.outcome === "skipped" && !item.title);
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
