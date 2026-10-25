import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Component and unit tests: jsdom for the DOM, Testing Library for queries.
// The contract harness (web/contract) has its own config and is not run here.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["./src/test/setup.ts"],
    restoreMocks: true,
    unstubGlobals: true,
    // Full-app tests render the router, the query cache and a page in jsdom,
    // which takes seconds per test on a loaded machine (CI, or every file at
    // once). The time limits bound a hang, not speed; the worker cap keeps
    // one run from starving itself. Testing Library's own wait limit is set
    // in src/test/setup.ts.
    testTimeout: 20_000,
    hookTimeout: 20_000,
    maxWorkers: "50%",
  },
});
