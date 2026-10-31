// Where the composed stack listens, and how to reach the parts the drills
// control. The defaults are what `mise run e2e:up` publishes on loopback.

export const stack = {
  /** The web app (nginx, which also proxies /api). */
  web: process.env.E2E_WEB_URL ?? "http://localhost:3000",
  /** The API's public listener, for /healthz and /readyz. */
  api: process.env.E2E_API_URL ?? "http://127.0.0.1:8095",
  /** The API's operator listener. */
  metrics: process.env.E2E_METRICS_URL ?? "http://127.0.0.1:9095/metrics",
  /** The metadata fixture server (fixtures/server.ts). */
  fixtures: process.env.E2E_FIXTURES_URL ?? "http://127.0.0.1:8090",
  /** Dex, as the browser reaches it. */
  dex: process.env.E2E_DEX_URL ?? "http://localhost:5556/dex",
  /**
   * Shell commands that stop and start PostgreSQL, for the database drill.
   * `mise run e2e` sets them to the compose stop / start of `postgres`.
   */
  stopDatabase: process.env.E2E_STOP_DATABASE,
  startDatabase: process.env.E2E_START_DATABASE,
  /** Where screenshots for review are written (a CI artifact). */
  screenshots: process.env.E2E_SCREENSHOTS_DIR ?? "artifacts/screenshots",
} as const;

/** The Dex static users (deploy/dex/config.yaml). */
export const users = {
  first: { email: "dev@stacks.local", password: "password", name: "dev" },
  second: {
    email: "friend@stacks.local",
    password: "password",
    name: "friend",
  },
} as const;

export type DexUser = (typeof users)[keyof typeof users];

/** The titles fixtures/server.ts serves. */
export const titles = {
  show: {
    ref: "tmdb:show:910001",
    name: "The Lighthouse Keepers",
    episodes: 4,
  },
  movie: { ref: "tmdb:movie:910002", name: "Paper Moons" },
  drama: { ref: "tmdb:show:910003", name: "Seoul Lanterns" },
  anime: { ref: "anilist:show:920001", name: "The Starlit Road" },
} as const;
