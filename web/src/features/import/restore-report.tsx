import type { RestoreReport, RestoreTally } from "@/api/schemas";

const TALLIES: readonly [keyof RestoreReport, string][] = [
  ["entries", "Library entries"],
  ["runs", "Watch runs"],
  ["plays", "Plays"],
  ["events", "History events"],
  ["lists", "Lists"],
  ["list_items", "List items"],
  ["tags", "Tags"],
  ["entry_tags", "Tagged titles"],
  ["blocklist", "Hidden titles"],
  ["notification_channels", "Notification channels"],
  ["notification_subscriptions", "Notification subscriptions"],
];

/** How many changed entries the diff lists before summing up the rest. */
const CHANGES_SHOWN = 50;

/** What a restore did, or with a dry run would do: counts, a diff and warnings. */
export function RestoreReportView({ report }: { report: RestoreReport }) {
  const changed = report.changes.filter(
    (c) => c.action === "inserted" || c.action === "updated",
  );
  const verb = report.dry_run ? "would be" : "were";
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-fg">
        {report.titles.matched + report.titles.created} titles:{" "}
        {report.titles.matched} already known, {report.titles.created} {verb}{" "}
        added to the catalog.
        {report.settings_changed ? ` Your settings ${verb} changed.` : ""}
      </p>
      <div className="overflow-x-auto">
        <table className="w-full min-w-[22rem] text-sm">
          <caption className="sr-only">
            Rows the restore{" "}
            {report.dry_run ? "would add or change" : "added or changed"}
          </caption>
          <thead>
            <tr className="text-left text-xs text-fg-muted">
              <th scope="col" className="py-1 pr-2 font-medium">
                Kind
              </th>
              <th scope="col" className="px-2 py-1 text-right font-medium">
                Added
              </th>
              <th scope="col" className="px-2 py-1 text-right font-medium">
                Changed
              </th>
              <th scope="col" className="px-2 py-1 text-right font-medium">
                Same
              </th>
              <th scope="col" className="py-1 pl-2 text-right font-medium">
                Skipped
              </th>
            </tr>
          </thead>
          <tbody>
            {TALLIES.map(([key, label]) => {
              const tally = report[key] as RestoreTally;
              return (
                <tr key={key} className="border-t border-line">
                  <th scope="row" className="py-1.5 pr-2 text-left font-normal">
                    {label}
                  </th>
                  <td className="px-2 py-1.5 text-right tabular-nums">
                    {tally.inserted}
                  </td>
                  <td className="px-2 py-1.5 text-right tabular-nums">
                    {tally.updated}
                  </td>
                  <td className="px-2 py-1.5 text-right tabular-nums text-fg-muted">
                    {tally.unchanged}
                  </td>
                  <td className="py-1.5 pl-2 text-right tabular-nums text-fg-muted">
                    {tally.skipped}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {changed.length > 0 && (
        <details className="text-sm">
          <summary className="cursor-pointer font-medium text-fg">
            {changed.length} library{" "}
            {changed.length === 1 ? "entry" : "entries"} {verb} added or changed
          </summary>
          <ul className="mt-2 flex flex-col gap-1">
            {changed.slice(0, CHANGES_SHOWN).map((change) => (
              <li key={change.title} className="flex justify-between gap-2">
                <code className="truncate">{change.title}</code>
                <span className="text-fg-muted">
                  {change.action === "inserted" ? "added" : "changed"}
                </span>
              </li>
            ))}
            {changed.length > CHANGES_SHOWN && (
              <li className="text-fg-muted">
                and {changed.length - CHANGES_SHOWN} more
              </li>
            )}
          </ul>
        </details>
      )}
      {report.warnings.length > 0 && (
        <div className="flex flex-col gap-1 rounded-md border border-warning/40 p-3 text-sm">
          <p className="font-medium text-fg">
            {report.warnings.length}{" "}
            {report.warnings.length === 1 ? "thing" : "things"}{" "}
            {report.dry_run ? "can't" : "couldn't"} be restored as it was
          </p>
          <ul className="list-disc pl-5 text-fg-muted">
            {report.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
