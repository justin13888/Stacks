// The v0.1.0 journey (docs/QA.md, "The journey"): one person tracks a show
// and a film through search, episodes, completion and a rewatch, organises
// them in a list and a tag, and exports a Stacks backup; a second person
// restores it into their empty account and ends with the same data.

import { readFile } from "node:fs/promises";
import { expect, test } from "@playwright/test";
import { signIn, signOut } from "./support/auth.ts";
import { type Backup, summarize } from "./support/backup.ts";
import { titles, users } from "./support/stack.ts";
import { choose, expectToast, progress, searchAndOpen } from "./support/ui.ts";

const LIST = "Weekend picks";
const TAG = "Comfort watch";
const NOTES = "Watch the finale with the lights off.";

test("track, organise, back up and restore into a fresh account", async ({
  page,
}, testInfo) => {
  test.setTimeout(300_000);

  await test.step("sign in through Dex", async () => {
    await signIn(page, users.first);
    await expect(
      page.getByRole("heading", { level: 1, name: "Up Next" }),
    ).toBeVisible();
  });

  await test.step("search and add the show to the library", async () => {
    await searchAndOpen(page, "lighthouse", titles.show.name);
    await choose(page, "Add to library", "Watching");
    await expect(page.getByRole("combobox", { name: "Status" })).toHaveText(
      /Watching/,
    );
    await expect(progress(page)).toHaveText("0 of 4 episodes");
  });

  await test.step("log episodes with +1 and mark-up-to", async () => {
    await page.getByRole("button", { name: "+1 episode" }).click();
    await expect(progress(page)).toHaveText("1 of 4 episodes");
    await page.getByRole("button", { name: "Mark up to S1E3 watched" }).click();
    await expect(progress(page)).toHaveText("3 of 4 episodes");
  });

  await test.step("finish the show", async () => {
    await page.getByRole("button", { name: "+1 episode" }).click();
    await expect(progress(page)).toHaveText("4 of 4 episodes");
    const status = page.getByRole("combobox", { name: "Status" });
    if (!/Completed/.test((await status.textContent()) ?? "")) {
      await choose(page, "Status", "Completed");
    }
    await expect(status).toHaveText(/Completed/);
  });

  await test.step("favourite it and write a note", async () => {
    // Exact: the Activity feed's "Undo: Added … to favourites" also matches.
    const favourite = page.getByRole("button", {
      name: "Favourite",
      exact: true,
    });
    await favourite.click();
    await expect(favourite).toHaveAttribute("aria-pressed", "true");
    await page.getByRole("textbox", { name: "Notes" }).fill(NOTES);
    await page.getByRole("textbox", { name: "Notes" }).blur();
    await expect(page.getByText("Saved", { exact: true })).toBeVisible();
  });

  await test.step("start a rewatch", async () => {
    await page.getByRole("button", { name: "Start a rewatch" }).click();
    await expect(page.getByText("Rewatch 1", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "+1 episode" }).click();
    await expect(progress(page)).toHaveText("1 of 4 episodes");
  });

  await test.step("tag the show", async () => {
    await page.getByRole("button", { name: "Tags", exact: true }).click();
    await page.getByRole("button", { name: "Manage tags" }).click();
    const manage = page.getByRole("dialog", { name: "Manage tags" });
    await manage
      .getByRole("textbox", { name: "New tag", exact: true })
      .fill(TAG);
    await manage.getByRole("button", { name: "Create" }).click();
    await expect(
      manage.getByRole("textbox", { name: `Name of ${TAG}` }),
    ).toBeVisible();
    await manage.getByRole("button", { name: "Close" }).click();
    await page.getByRole("button", { name: "Tags", exact: true }).click();
    const assign = page.getByRole("dialog", { name: "Tags" });
    const chip = assign.getByRole("button", { name: TAG, exact: true });
    await chip.click();
    await expectToast(page, `Tagged ${TAG}`);
    await expect(chip).toHaveAttribute("aria-pressed", "true");
    await assign.getByRole("button", { name: "Close" }).click();
    await expect(assign).toBeHidden();
  });

  await test.step("watch the film", async () => {
    await searchAndOpen(page, "paper", titles.movie.name);
    await page.getByRole("button", { name: "Mark watched" }).click();
    await expect(page.getByText("Watched", { exact: true })).toBeVisible();
  });

  await test.step("create a list and add both titles", async () => {
    await page.getByRole("link", { name: "Lists" }).click();
    await page.getByRole("button", { name: "New list" }).first().click();
    const dialog = page.getByRole("dialog", { name: "New list" });
    await dialog.getByRole("textbox", { name: "Name" }).fill(LIST);
    await dialog.getByRole("button", { name: /Create|Save/ }).click();
    await expect(page.getByText(LIST).first()).toBeVisible();

    for (const title of [titles.show, titles.movie]) {
      await page.goto(`/titles/${encodeURIComponent(title.ref)}`);
      await page.getByRole("button", { name: "Add to list" }).click();
      await page
        .getByRole("dialog", { name: "Add to a list" })
        .getByRole("button", { name: new RegExp(LIST) })
        .click();
      await expectToast(page, `Added to ${LIST}`);
    }
  });

  await test.step("reorder the list", async () => {
    await page.getByRole("link", { name: "Lists" }).click();
    await page
      .getByRole("link", { name: new RegExp(LIST) })
      .first()
      .click();
    const items = page.getByRole("listitem").filter({
      has: page.getByRole("button", { name: /^Move .* (up|down)$/ }),
    });
    await expect(items.first()).toContainText(titles.show.name);
    await page
      .getByRole("button", { name: `Move ${titles.movie.name} up` })
      .click();
    await expect(items.first()).toContainText(titles.movie.name);
    await page.reload();
    await expect(items.first()).toContainText(titles.movie.name);
  });

  let exported: Backup;
  await test.step("export a Stacks backup", async () => {
    await page.goto("/settings/data");
    const download = page.waitForEvent("download");
    await page.getByRole("link", { name: "Download backup" }).click();
    const file = testInfo.outputPath("stacks-backup.json");
    await (await download).saveAs(file);
    exported = JSON.parse(await readFile(file, "utf8")) as Backup;
    expect(exported.format).toBe("stacks-backup");
    expect(exported.entries).toHaveLength(2);
    await testInfo.attach("backup", {
      path: file,
      contentType: "application/json",
    });
  });

  await test.step("restore it into a fresh account", async () => {
    await signOut(page, users.first);
    await signIn(page, users.second);
    await page.goto("/library");
    await expect(page.getByText(titles.show.name)).toHaveCount(0);

    await page.goto("/settings/import");
    await page
      .getByLabel("File")
      .setInputFiles(testInfo.outputPath("stacks-backup.json"));
    await page.getByRole("button", { name: "Preview restore" }).click();
    await expect(
      page.getByRole("region", { name: "Restore preview" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Restore backup" }).click();
    await page
      .getByRole("dialog", { name: "Restore this backup?" })
      .getByRole("button", { name: "Restore" })
      .click();
    await expect(
      page.getByRole("region", { name: "Restore summary" }),
    ).toBeVisible();
  });

  await test.step("the second account holds the same data", async () => {
    await page.goto("/library");
    await expect(page.getByText(titles.show.name).first()).toBeVisible();
    await expect(page.getByText(titles.movie.name).first()).toBeVisible();

    const res = await page.request.get("/api/v1/exports/backup");
    expect(res.ok()).toBe(true);
    const restored = (await res.json()) as Backup;
    expect(summarize(restored)).toEqual(summarize(exported));
  });
});
