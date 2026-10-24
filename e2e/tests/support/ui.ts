import { expect, type Page } from "@playwright/test";

/** Opens the ⌘K palette, searches, and opens the result named `name`. */
export async function searchAndOpen(
  page: Page,
  query: string,
  name: string,
): Promise<void> {
  await page.getByRole("button", { name: /Search titles/ }).click();
  const dialog = page.getByRole("dialog", { name: "Search titles" });
  await dialog.getByRole("combobox", { name: "Search titles" }).fill(query);
  await dialog
    .getByRole("option", { name: new RegExp(`^${name}`) })
    .first()
    .click();
  await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
}

/** Picks `option` in the Radix select labelled `label`. */
export async function choose(
  page: Page,
  label: string,
  option: string,
): Promise<void> {
  await page.getByRole("combobox", { name: label }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
}

/** The episode progress line on a show's title page ("3 of 4 episodes"). */
export function progress(page: Page) {
  return page.getByText(/^\d+ of \d+ episodes$/);
}

/**
 * Waits for a toast with `title`. Toasts live in Radix's F8 region, which an
 * open dialog hides from the accessibility tree, so it is found by its label
 * rather than by role.
 */
export async function expectToast(page: Page, title: string | RegExp) {
  await expect(
    page.locator('[aria-label^="Notifications"]').getByText(title).first(),
  ).toBeVisible();
}
