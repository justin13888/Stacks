// What a Stacks backup (docs/backup-format.md) says about an account, with
// what legitimately differs between two accounts that hold the same data
// taken out: row ids, the account's own profile, when the file was written,
// and the event log (a restore appends events of its own). Two accounts whose
// summaries are equal hold the same library, runs, plays, lists in the same
// order, tags, blocklist and settings.

interface TitleRef {
  provider: string;
  kind: string;
  external_id: string;
  primary: boolean;
}

interface Title {
  id: string;
  name: string;
  refs: TitleRef[];
}

interface Play {
  episode: { season: number; number: number } | null;
  watched_at: string | null;
  source: string;
  voided_at: string | null;
}

interface Run {
  number: number;
  started_on: string | null;
  started_precision: string | null;
  completed_on: string | null;
  completed_precision: string | null;
  plays: Play[];
}

interface Entry {
  title_id: string;
  status: string;
  score: number | null;
  favourite: boolean;
  notes: string | null;
  deleted_at: string | null;
  runs: Run[];
  tags: { tag_id: string }[];
}

interface List {
  name: string;
  description: string | null;
  smart: boolean;
  deleted_at: string | null;
  items: { title_id: string; notes: string | null }[];
}

interface Tag {
  id: string;
  name: string;
  color: string | null;
}

export interface Backup {
  format: string;
  format_version: number;
  settings: Record<string, unknown>;
  titles: Title[];
  entries: Entry[];
  lists: List[];
  tags: Tag[];
  blocklist: { title_id: string }[];
}

/** The public id of a title (`tmdb:show:910001`). */
function publicRef(title: Title): string {
  const ref = title.refs.find((r) => r.primary) ?? title.refs[0];
  return ref ? `${ref.provider}:${ref.kind}:${ref.external_id}` : title.name;
}

export function summarize(backup: Backup) {
  const refs = new Map(backup.titles.map((t) => [t.id, publicRef(t)]));
  const ref = (id: string) => refs.get(id) ?? `missing title ${id}`;
  const tagNames = new Map(backup.tags.map((t) => [t.id, t.name]));
  const byKey = <T extends { key: string }>(a: T, b: T) =>
    a.key.localeCompare(b.key);

  return {
    settings: backup.settings,
    entries: backup.entries
      .map((e) => ({
        key: ref(e.title_id),
        status: e.status,
        score: e.score,
        favourite: e.favourite,
        notes: e.notes,
        deleted: e.deleted_at !== null,
        tags: e.tags.map((t) => tagNames.get(t.tag_id) ?? t.tag_id).sort(),
        runs: [...e.runs]
          .sort((a, b) => a.number - b.number)
          .map((r) => ({
            number: r.number,
            started: [r.started_on, r.started_precision],
            completed: [r.completed_on, r.completed_precision],
            plays: r.plays
              .map((p) => ({
                episode: p.episode
                  ? `S${p.episode.season}E${p.episode.number}`
                  : "movie",
                watched_at: p.watched_at,
                source: p.source,
                voided: p.voided_at !== null,
              }))
              .sort((a, b) =>
                `${a.episode}${a.watched_at}`.localeCompare(
                  `${b.episode}${b.watched_at}`,
                ),
              ),
          })),
      }))
      .sort(byKey),
    lists: backup.lists
      .map((l) => ({
        key: l.name,
        description: l.description,
        smart: l.smart,
        deleted: l.deleted_at !== null,
        // In list order: the backup writes items by position.
        items: l.items.map((i) => ({ title: ref(i.title_id), notes: i.notes })),
      }))
      .sort(byKey),
    tags: backup.tags.map((t) => ({ key: t.name, color: t.color })).sort(byKey),
    blocklist: backup.blocklist.map((b) => ref(b.title_id)).sort(),
  };
}
