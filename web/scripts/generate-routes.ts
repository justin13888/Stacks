/**
 * Generate `src/routeTree.gen.ts` outside of Vite.
 *
 * The TanStack Router Vite plugin only writes the route tree while `vite dev`
 * or `vite build` is running, but `tsc` and Vitest both need it — and it is
 * gitignored, so a clean checkout has none. Running the same generator the
 * plugin uses keeps the file generated (not committed) while making it
 * available to every consumer.
 */
import { Generator, getConfig } from "@tanstack/router-generator";

const config = await getConfig({ routesDirectory: "src/router/routes" });
await new Generator({ config, root: process.cwd() }).run();

console.log(`Wrote ${config.generatedRouteTree}`);
