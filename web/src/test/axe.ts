// An automated accessibility check for page tests (axe-core under jsdom).
// Test-only: no page imports it. jsdom does no layout or painting, so the
// rules that need them (colour contrast) cannot run here and are left to
// the browser.

import axe from "axe-core";
import { expect } from "vitest";

/** Fails the test on any `serious` or `critical` axe violation in `root`. */
export async function expectNoSeriousA11yViolations(
  root: Element,
): Promise<void> {
  const results = await axe.run(root, {
    rules: { "color-contrast": { enabled: false } },
  });
  const serious = results.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map(
      (v) =>
        `${v.id}: ${v.help} (${v.nodes.map((n) => n.target.join(" ")).join(", ")})`,
    );
  expect(serious).toEqual([]);
}
