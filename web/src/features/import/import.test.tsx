// The import wizard in the real app: every source, the backup dry run, the
// progress of a queued import, and resolving its unmatched rows.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type {
  Import,
  ImportDetail,
  ImportItem,
  ImportSource,
  RestoreReport,
  TitleSummary,
} from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/features/library/axe";
import {
  json,
  makeSession,
  mockApi,
  renderApp,
  requestsOf,
} from "@/test/utils";

const IMPORT_ID = "0190a8e0-0000-7000-8000-0000000000i1";

const EVA: TitleSummary = {
  ref: "anilist:show:30",
  title: "Neon Genesis Evangelion",
  kind: "show",
  category: "anime",
  year: 1995,
  genres: [],
};

const NO_TALLY = { inserted: 0, updated: 0, unchanged: 0, skipped: 0 };

function report(dryRun: boolean): RestoreReport {
  return {
    read_version: 1,
    mode: "merge",
    dry_run: dryRun,
    settings_changed: false,
    titles: { matched: 3, created: 1 },
    entries: { inserted: 2, updated: 1, unchanged: 1, skipped: 0 },
    runs: NO_TALLY,
    plays: { inserted: 12, updated: 0, unchanged: 0, skipped: 0 },
    events: NO_TALLY,
    entry_tags: NO_TALLY,
    lists: NO_TALLY,
    list_items: NO_TALLY,
    tags: NO_TALLY,
    blocklist: NO_TALLY,
    notification_channels: NO_TALLY,
    notification_subscriptions: {
      inserted: 0,
      updated: 0,
      unchanged: 0,
      skipped: 1,
    },
    changes: [
      { title: "tmdb:movie:550", action: "inserted" },
      { title: "anilist:show:21", action: "updated" },
      { title: "tmdb:movie:603", action: "unchanged" },
    ],
    warnings: ["A channel without its configuration was skipped."],
  };
}

const UNMATCHED: ImportItem[] = [
  {
    row: 1,
    outcome: "unmatched",
    source: {
      title: "Evangelion",
      source_id: "mal:30",
      kind: "show",
      year: 1995,
    },
    candidates: [
      {
        title: EVA.ref,
        name: "Neon Genesis Evangelion",
        kind: "show",
        year: 1995,
        confidence: 0.62,
      },
      {
        title: "anilist:movie:32",
        name: "The End of Evangelion",
        kind: "movie",
        year: 1997,
        confidence: 0.4,
      },
    ],
  },
  {
    row: 2,
    outcome: "unmatched",
    source: { title: "Eva Rebuild", source_id: "mal:2759", year: 2007 },
    candidates: [],
  },
  {
    row: 3,
    outcome: "unmatched",
    source: { title: "Some obscure OVA", source_id: "mal:99999" },
    candidates: [],
  },
  {
    row: 4,
    outcome: "applied",
    source: { title: "Cowboy Bebop" },
    title: { ...EVA, ref: "anilist:show:1", title: "Cowboy Bebop" },
    confidence: 0.55,
    candidates: [],
  },
];

function importOf(
  source: ImportSource,
  status: Import["status"],
  items: ImportItem[],
): ImportDetail {
  const count = (outcome: ImportItem["outcome"]) =>
    items.filter((i) => i.outcome === outcome).length;
  return {
    id: IMPORT_ID,
    source,
    status,
    conflict_strategy: "skip",
    filename: "animelist.xml",
    size: 1024,
    counts: {
      total: items.length,
      applied: count("applied"),
      skipped: count("skipped"),
      unmatched: count("unmatched"),
      failed: count("failed"),
      pending_retry: 0,
    },
    created_at: "2026-10-20T12:00:00Z",
    started_at: "2026-10-20T12:00:01Z",
    completed_at: status === "completed" ? "2026-10-20T12:00:09Z" : null,
    items,
  };
}

interface Upload {
  path: string;
  query: string;
  fields: Record<string, string>;
  /** The file part's contents. */
  file: string | null;
}

interface Resolution {
  path: string;
  body: unknown;
}

/**
 * The imports API. `gets` lists, in order, what each `GET /imports/{id}`
 * answers (the last one repeats), so a test can watch an import progress.
 */
function importServer(options: { gets?: ImportDetail[] } = {}) {
  const uploads: Upload[] = [];
  const resolutions: Resolution[] = [];
  let items = options.gets?.at(-1)?.items ?? UNMATCHED;
  let reads = 0;
  const api = mockApi(async (request) => {
    const url = new URL(request.url);
    const path = url.pathname.replace(/^\/api\/v1/, "");
    const method = request.method;
    if (method === "GET" && path === "/sessions/current") {
      return json(makeSession());
    }
    if (
      method === "POST" &&
      (path === "/imports" || path === "/imports/backup")
    ) {
      const form = await request.formData();
      const fields: Record<string, string> = {};
      let file: string | null = null;
      for (const [name, value] of form.entries()) {
        if (typeof value === "string") fields[name] = value;
        // jsdom's File loses its name on the way through the fetch stub, so
        // the part is told apart by its contents.
        else file = await value.text();
      }
      uploads.push({ path, query: url.search, fields, file });
      if (path === "/imports/backup") {
        return json(report(url.searchParams.get("dry_run") === "true"));
      }
      const { items: _items, ...queued } = importOf(
        fields.source as ImportSource,
        "pending",
        [],
      );
      return json(queued, 202, { location: `/api/v1/imports/${IMPORT_ID}` });
    }
    if (method === "GET" && path === "/imports") {
      return json({
        items: uploads.length ? [importOf("mal_xml", "completed", items)] : [],
        next_cursor: null,
      });
    }
    if (method === "GET" && path === `/imports/${IMPORT_ID}`) {
      const answer = options.gets?.[Math.min(reads, options.gets.length - 1)];
      reads += 1;
      if (answer && answer.status !== "completed") return json(answer);
      return json(importOf("mal_xml", "completed", items));
    }
    const resolve = path.match(/^\/imports\/[^/]+\/items\/(\d+)$/);
    if (method === "PATCH" && resolve) {
      const body = await request.json();
      resolutions.push({ path, body });
      const row = Number(resolve[1]);
      const title: TitleSummary = { ...EVA, ref: body.title };
      const resolved: ImportItem = {
        row,
        outcome: "applied",
        source: items.find((i) => i.row === row)?.source,
        title,
        confidence: 1,
        candidates: [],
      };
      items = items.map((i) => (i.row === row ? resolved : i));
      return json(resolved);
    }
    if (method === "GET" && path === "/search") {
      return json({
        items: [{ ...EVA, ref: "anilist:show:2759", title: "Evangelion: 1.0" }],
        degraded: [],
        unreadable: 0,
      });
    }
    return undefined;
  });
  return { api, uploads, resolutions };
}

function fileOf(name: string, type: string) {
  return new File([`contents of ${name}`], name, { type });
}

async function pickSource(label: string) {
  await userEvent.click(
    await screen.findByRole(
      "radio",
      { name: new RegExp(label) },
      { timeout: 5000 },
    ),
  );
}

describe("import wizard", () => {
  test.each([
    ["MyAnimeList XML", "mal_xml", "animelist.xml", "text/xml"],
    ["MyAnimeList JSON", "mal_json", "animelist.json", "application/json"],
    ["TMDB JSON", "tmdb_json", "tmdb.json", "application/json"],
  ] as const)(
    "%s is uploaded as an import and its summary shows",
    async (label, source, filename, type) => {
      const server = importServer();
      const { router } = renderApp("/settings/import");
      await pickSource(label);
      await userEvent.upload(
        screen.getByLabelText("File"),
        fileOf(filename, type),
      );
      await userEvent.click(
        screen.getByRole("combobox", {
          name: "Titles already in your library",
        }),
      );
      await userEvent.click(
        await screen.findByRole("option", { name: /^Merge/ }),
      );
      await userEvent.click(
        screen.getByRole("button", { name: "Start import" }),
      );

      const summary = await screen.findByLabelText(
        "Import summary",
        undefined,
        {
          timeout: 5000,
        },
      );
      expect(server.uploads).toEqual([
        {
          path: "/imports",
          query: "",
          fields: { source, conflict_strategy: "merge" },
          file: `contents of ${filename}`,
        },
      ]);
      // The import's address is in the URL, so a reload comes back to it.
      expect(router.state.location.search).toEqual({ import: IMPORT_ID });
      expect(
        within(summary).getByText("Unmatched").nextSibling,
      ).toHaveTextContent("3");
    },
  );

  test("a running import is polled until it finishes", async () => {
    importServer({
      gets: [
        {
          ...importOf("mal_xml", "running", []),
          counts: {
            total: 4,
            applied: 1,
            skipped: 0,
            unmatched: 0,
            failed: 0,
            pending_retry: 0,
          },
        },
        importOf("mal_xml", "completed", UNMATCHED),
      ],
    });
    renderApp(`/settings/import?import=${IMPORT_ID}`);
    expect(
      await screen.findByText("1 of 4 rows read", undefined, { timeout: 5000 }),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole(
        "heading",
        { name: "3 rows need a title" },
        {
          timeout: 5000,
        },
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("1 of 4 rows read")).not.toBeInTheDocument();
  });

  test("a backup is previewed with a dry run and written only once confirmed", async () => {
    const server = importServer();
    renderApp("/settings/import");
    await pickSource("Stacks backup");
    await userEvent.upload(
      screen.getByLabelText("File"),
      fileOf("stacks-backup.json", "application/json"),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Preview restore" }),
    );

    const preview = await screen.findByRole("region", {
      name: "Restore preview",
    });
    expect(preview).toHaveTextContent("nothing has changed yet");
    expect(
      within(preview).getByRole("row", { name: /Library entries 2 1 1 0/ }),
    ).toBeInTheDocument();
    expect(preview).toHaveTextContent(
      "2 library entries would be added or changed",
    );
    expect(preview).toHaveTextContent(
      "A channel without its configuration was skipped.",
    );
    expect(server.uploads.map((u) => u.query)).toEqual([
      "?mode=merge&dry_run=true",
    ]);

    // Asking to restore opens a confirmation; cancelling writes nothing.
    await userEvent.click(
      within(preview).getByRole("button", { name: "Restore backup" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Restore this backup?",
    });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Cancel" }),
    );
    expect(server.uploads).toHaveLength(1);

    await userEvent.click(
      within(preview).getByRole("button", { name: "Restore backup" }),
    );
    await userEvent.click(
      within(
        await screen.findByRole("dialog", { name: "Restore this backup?" }),
      ).getByRole("button", { name: "Restore" }),
    );
    expect(
      await screen.findByRole("region", { name: "Restore summary" }),
    ).toHaveTextContent("2 library entries were added or changed");
    expect(server.uploads.map((u) => [u.query, u.file])).toEqual([
      ["?mode=merge&dry_run=true", "contents of stacks-backup.json"],
      ["?mode=merge", "contents of stacks-backup.json"],
    ]);
  });

  test("changing the restore mode discards the preview", async () => {
    const server = importServer();
    renderApp("/settings/import");
    await pickSource("Stacks backup");
    await userEvent.upload(
      screen.getByLabelText("File"),
      fileOf("stacks-backup.json", "application/json"),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Preview restore" }),
    );
    await screen.findByRole("region", { name: "Restore preview" });
    await userEvent.click(
      screen.getByRole("combobox", { name: "Restore mode" }),
    );
    await userEvent.click(
      await screen.findByRole("option", { name: /^Overwrite/ }),
    );
    expect(
      screen.queryByRole("region", { name: "Restore preview" }),
    ).not.toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Preview restore" }),
    );
    await screen.findByRole("region", { name: "Restore preview" });
    expect(server.uploads.map((u) => u.query)).toEqual([
      "?mode=merge&dry_run=true",
      "?mode=overwrite&dry_run=true",
    ]);
  });
});

describe("match resolution", () => {
  test("accepting a candidate, a search result and skipping each do the right thing", async () => {
    const server = importServer();
    renderApp(`/settings/import?import=${IMPORT_ID}`);
    await screen.findByRole(
      "heading",
      { name: "3 rows need a title" },
      {
        timeout: 5000,
      },
    );

    // The row's raw values and candidates are shown.
    const first = screen.getByRole("listitem", { name: "Evangelion" });
    expect(first).toHaveTextContent("mal:30");
    expect(first).toHaveTextContent("62% match");
    await userEvent.click(
      within(first).getByRole("button", {
        name: "Use Neon Genesis Evangelion for Evangelion",
      }),
    );
    await waitFor(() =>
      expect(server.resolutions).toEqual([
        { path: `/imports/${IMPORT_ID}/items/1`, body: { title: EVA.ref } },
      ]),
    );
    await screen.findByRole("heading", { name: "2 rows need a title" });

    // A row searches the catalog, seeded with its title, only when asked:
    // nothing was searched for until then.
    expect(requestsOf(server.api).some((r) => r.includes("/search"))).toBe(
      false,
    );
    const second = screen.getByRole("listitem", { name: "Eva Rebuild" });
    await userEvent.click(
      within(second).getByRole("button", { name: "Search" }),
    );
    const search = within(second).getByRole("combobox", {
      name: "Search for Eva Rebuild",
    });
    expect(search).toHaveValue("Eva Rebuild");
    await userEvent.click(
      await within(second).findByRole("option", { name: "Evangelion: 1.0" }),
    );
    await waitFor(() =>
      expect(server.resolutions.at(-1)).toEqual({
        path: `/imports/${IMPORT_ID}/items/2`,
        body: { title: "anilist:show:2759" },
      }),
    );

    // Skip sets a row aside without asking the API.
    const third = await screen.findByRole("listitem", {
      name: "Some obscure OVA",
    });
    await userEvent.click(
      within(third).getByRole("button", { name: "Skip Some obscure OVA" }),
    );
    expect(
      screen.queryByRole("listitem", { name: "Some obscure OVA" }),
    ).not.toBeInTheDocument();
    expect(server.resolutions).toHaveLength(2);
    expect(screen.getByText(/Skipped rows stay unmatched/)).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Show 1 skipped" }),
    );
    expect(
      screen.getByRole("listitem", { name: "Some obscure OVA" }),
    ).toBeInTheDocument();

    // A row matched with little certainty is listed for checking.
    expect(
      screen.getByRole("region", { name: "Low-confidence matches" }),
    ).toHaveTextContent("55% sure");
  });

  test("the import page has no serious accessibility violations", async () => {
    importServer();
    const { container } = renderApp(`/settings/import?import=${IMPORT_ID}`);
    await screen.findByRole(
      "heading",
      { name: "3 rows need a title" },
      {
        timeout: 5000,
      },
    );
    await expectNoSeriousA11yViolations(container);
  });

  test("the wizard has no serious accessibility violations", async () => {
    importServer();
    const { container } = renderApp("/settings/import");
    await pickSource("Stacks backup");
    await expectNoSeriousA11yViolations(container);
  });
});
