// Vitest global setup: serve the kynos contract harness on an ephemeral port
// for the round-trip suite, and stop it afterwards.
//
// `cargo run` rather than a fixed binary path, so CARGO_TARGET_DIR and a
// stale build are both handled; `mise run contract` has already built the
// example while exporting the spec, so this only links and starts it.

import { type ChildProcess, spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import type { TestProject } from "vitest/node";

declare module "vitest" {
  export interface ProvidedContext {
    baseUrl: string;
  }
}

const manifest = fileURLToPath(
  new URL("../../../api/Cargo.toml", import.meta.url),
);

/** How long to wait for the harness to report its address. */
const STARTUP_TIMEOUT_MS = 180_000;

function start(): Promise<{ child: ChildProcess; baseUrl: string }> {
  const child = spawn(
    "cargo",
    [
      "run",
      "--quiet",
      "--manifest-path",
      manifest,
      "--example",
      "contract",
      "--",
      "serve",
    ],
    { stdio: ["ignore", "pipe", "inherit"] },
  );
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill();
      reject(
        new Error(`harness did not start within ${STARTUP_TIMEOUT_MS} ms`),
      );
    }, STARTUP_TIMEOUT_MS);
    let output = "";
    child.stdout?.setEncoding("utf8");
    child.stdout?.on("data", (chunk: string) => {
      output += chunk;
      const match = /listening on (http:\/\/\S+)/.exec(output);
      if (match?.[1] !== undefined) {
        clearTimeout(timer);
        resolve({ child, baseUrl: match[1] });
      }
    });
    child.on("exit", (code, signal) => {
      clearTimeout(timer);
      reject(new Error(`harness exited before listening (${code ?? signal})`));
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
  });
}

export default async function setup(project: TestProject) {
  const { child, baseUrl } = await start();
  project.provide("baseUrl", baseUrl);
  return () => {
    child.kill();
  };
}
