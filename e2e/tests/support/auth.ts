import { expect, type Page } from "@playwright/test";
import type { DexUser } from "./stack.ts";

/**
 * Signs in the way a person does: Stacks' sign-in button, Dex's own login
 * form, and back through the callback. Nothing is bypassed; the session is
 * the cookie the API sets after verifying Dex's id_token.
 */
export async function signIn(page: Page, user: DexUser): Promise<void> {
  await page.goto("/login");
  await page.getByRole("button", { name: "Sign in" }).click();
  // Dex's password connector form.
  await page.waitForURL(/\/dex\//);
  await page.locator("#login").fill(user.email);
  await page.locator("#password").fill(user.password);
  await page.locator("#submit-login").click();
  await page.waitForURL(
    (url) =>
      !url.pathname.startsWith("/auth/callback") && !url.href.includes("/dex/"),
  );
  await expect(
    page
      .getByRole("button", {
        name: new RegExp(`Account menu for ${user.name}`, "i"),
      })
      .first(),
  ).toBeVisible();
}

/** Signs out through the account menu, back to the sign-in page. */
export async function signOut(page: Page, user: DexUser): Promise<void> {
  await page
    .getByRole("button", {
      name: new RegExp(`Account menu for ${user.name}`, "i"),
    })
    .first()
    .click();
  await page.getByRole("menuitem", { name: "Sign out" }).click();
  await expect(page.getByTestId("login-page")).toBeVisible();
}
