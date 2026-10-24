import { defineConfig, devices } from "@playwright/test";
import { stack } from "./tests/support/stack.ts";

// The suite drives one shared, stateful stack (`just e2e-up`), so it runs in
// a fixed order on one worker: the journey builds the library, the screens
// pass photographs it, and the drills break the upstreams last. A retry would
// replay the journey over the data its first attempt left, so there is none;
// the trace of a failure is kept instead.
export default defineConfig({
  testDir: "tests",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: !!process.env.CI,
  timeout: 90_000,
  expect: { timeout: 15_000 },
  outputDir: "test-results",
  reporter: [
    ["list"],
    ["html", { open: "never", outputFolder: "playwright-report" }],
  ],
  globalSetup: "./tests/support/global-setup.ts",
  use: {
    ...devices["Desktop Chrome"],
    baseURL: stack.web,
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    // The suite reads text the way a person does; keep the locale fixed so
    // dates and numbers render the same on every machine.
    locale: "en-US",
    timezoneId: "UTC",
  },
  projects: [
    { name: "journey", testMatch: /journey\.spec\.ts/ },
    {
      name: "screens",
      testMatch: /screens\.spec\.ts/,
      dependencies: ["journey"],
    },
    {
      name: "drills",
      testMatch: /drills\.spec\.ts/,
      dependencies: ["screens"],
    },
  ],
});
