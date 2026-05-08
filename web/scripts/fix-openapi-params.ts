#!/usr/bin/env bun
/**
 * Patch the OpenAPI spec produced by `gen-openapi` so every path with
 * `{name}` placeholders has a matching `parameters: [{ in: "path", … }]`
 * entry on each operation. Salvo does not auto-emit those when handlers
 * read params via `req.param::<T>("name")`, and orval refuses to generate
 * a client for any operation whose path placeholders aren't declared.
 *
 * Idempotent: re-running it on an already-patched spec is a no-op.
 *
 * Usage: bun web/scripts/fix-openapi-params.ts <openapi.json>
 */

import { readFileSync, writeFileSync } from "node:fs";

const target = process.argv[2];
if (!target) {
  console.error("usage: fix-openapi-params.ts <openapi.json>");
  process.exit(1);
}

type Param = {
  name: string;
  in: string;
  required?: boolean;
  schema?: { type: string };
};

type Operation = {
  parameters?: Param[];
  [k: string]: unknown;
};

type Spec = {
  paths: Record<string, Record<string, Operation>>;
  [k: string]: unknown;
};

const spec = JSON.parse(readFileSync(target, "utf8")) as Spec;
const verbs = ["get", "post", "put", "delete", "patch", "head", "options"];

let patched = 0;
for (const [path, ops] of Object.entries(spec.paths)) {
  const placeholders = [...path.matchAll(/\{([^/}]+)\}/g)].map((m) => m[1]);
  if (placeholders.length === 0) continue;

  for (const verb of verbs) {
    const op = ops[verb] as Operation | undefined;
    if (!op) continue;

    const declared = new Set(
      (op.parameters ?? []).filter((p) => p.in === "path").map((p) => p.name),
    );

    const missing = placeholders.filter((n) => !declared.has(n));
    if (missing.length === 0) continue;

    op.parameters = op.parameters ?? [];
    for (const name of missing) {
      op.parameters.push({
        name,
        in: "path",
        required: true,
        schema: { type: "string" },
      });
      patched += 1;
    }
  }
}

writeFileSync(target, `${JSON.stringify(spec, null, 2)}\n`);
console.log(`fix-openapi-params: patched ${patched} parameter(s)`);
