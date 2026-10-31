import { stack } from "./stack.ts";

/** How long the stack gets to come up before the suite gives up. */
const READY_WITHIN_MS = 180_000;

async function answers(url: string): Promise<boolean> {
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(5_000) });
    return res.ok;
  } catch {
    return false;
  }
}

/**
 * Waits, up to READY_WITHIN_MS, for every part of the stack the suite talks
 * to, then puts both metadata upstreams back to serving fixtures (a previous
 * run's drill may have left one broken).
 */
export default async function globalSetup(): Promise<void> {
  const probes = {
    "web app": stack.web,
    "API readiness": `${stack.api}/readyz`,
    "metadata fixtures": `${stack.fixtures}/__health`,
    Dex: `${stack.dex}/.well-known/openid-configuration`,
  };
  const deadline = Date.now() + READY_WITHIN_MS;
  for (const [name, url] of Object.entries(probes)) {
    while (!(await answers(url))) {
      if (Date.now() > deadline) {
        throw new Error(
          `${name} (${url}) did not answer within ${READY_WITHIN_MS / 1000}s; is the stack up (mise run e2e:up)?`,
        );
      }
      await new Promise((resolve) => setTimeout(resolve, 2_000));
    }
  }
  for (const upstream of ["tmdb", "anilist"]) {
    const res = await fetch(`${stack.fixtures}/__control/${upstream}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ mode: "ok" }),
    });
    if (!res.ok) throw new Error(`could not reset the ${upstream} fixture`);
  }
}
