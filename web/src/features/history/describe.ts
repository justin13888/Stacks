// History events in words ("Watched S1E3 of Frieren", "Score 8.0 → 9.0"),
// and the inverse each undoable one has.

import type {
  Change,
  EntryChanges,
  EntrySettings,
  Play,
  ScoreFormat,
} from "@/api/schemas";
import { STATUS_LABELS } from "@/components/ui/status-badge";
import { episodeLabel } from "@/features/titles/episodes";
import { formatScore } from "@/lib/score-format";

/** What undoing an event does. */
export type Undo =
  | { kind: "void-play"; playId: string }
  | { kind: "patch-entry"; changes: EntryChanges };

export interface Described {
  /** One line per thing the event changed. */
  lines: string[];
  undo?: Undo;
}

function playWords(play: Play, name: string): string {
  if (play.season == null || play.episode == null) return name;
  return `${episodeLabel({ season: play.season, episode: play.episode })} of ${name}`;
}

function score(value: number | null | undefined, format: ScoreFormat): string {
  return value == null ? "none" : formatScore(value, format);
}

/** The lines an entry update reads as, and the patch that puts it back. */
function describeUpdate(
  before: EntrySettings,
  after: EntrySettings,
  name: string,
  format: ScoreFormat,
): Described {
  const lines: string[] = [];
  const inverse: EntryChanges = {};
  if (before.status !== after.status) {
    lines.push(
      `Changed the status of ${name}: ${STATUS_LABELS[before.status]} → ${STATUS_LABELS[after.status]}`,
    );
    inverse.status = before.status;
  }
  if ((before.score ?? null) !== (after.score ?? null)) {
    lines.push(
      before.score == null
        ? `Scored ${name} ${score(after.score, format)}`
        : after.score == null
          ? `Cleared the score of ${name} (was ${score(before.score, format)})`
          : `Changed the score of ${name}: ${score(before.score, format)} → ${score(after.score, format)}`,
    );
    inverse.score = before.score ?? null;
  }
  if (before.favourite !== after.favourite) {
    lines.push(
      after.favourite
        ? `Added ${name} to favourites`
        : `Removed ${name} from favourites`,
    );
    inverse.favourite = before.favourite;
  }
  if ((before.notes ?? null) !== (after.notes ?? null)) {
    lines.push(
      after.notes
        ? `Edited the notes on ${name}`
        : `Cleared the notes on ${name}`,
    );
    inverse.notes = before.notes ?? null;
  }
  if (lines.length === 0) return { lines: [`Updated ${name}`] };
  return { lines, undo: { kind: "patch-entry", changes: inverse } };
}

/**
 * `change` in words about the title `name`. `voided` holds the plays already
 * undone, whose "watched" events offer no undo.
 */
export function describeChange(
  change: Change,
  name: string,
  format: ScoreFormat,
  voided: ReadonlySet<string>,
): Described {
  switch (change.kind) {
    case "entry_created":
      return {
        lines: [`Added ${name} to ${STATUS_LABELS[change.entry.status]}`],
      };
    case "entry_updated":
      return describeUpdate(change.before, change.after, name, format);
    case "entry_deleted":
      return { lines: [`Moved ${name} to the trash`] };
    case "entry_restored":
      return { lines: [`Restored ${name} from the trash`] };
    case "run_started":
      return {
        lines: [
          change.run.number > 1
            ? `Started rewatch ${change.run.number - 1} of ${name}`
            : `Started watching ${name}`,
        ],
      };
    case "run_updated":
      return {
        lines: [
          change.after.completed && !change.before.completed
            ? `Finished run ${change.after.number} of ${name}`
            : `Changed the dates of run ${change.after.number} of ${name}`,
        ],
      };
    case "play_logged": {
      const undone =
        change.play.voided_at != null || voided.has(change.play.id);
      return {
        lines: [`Watched ${playWords(change.play, name)}`],
        undo: undone
          ? undefined
          : { kind: "void-play", playId: change.play.id },
      };
    }
    case "play_voided":
      return { lines: [`Undid watching ${playWords(change.play, name)}`] };
  }
}
