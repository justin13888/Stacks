# ADR 0007: The web stack

- Status: accepted
- Date: 2026-10-23
- Issue: #8

## Context

The web client was written against the Salvo API. Its pages hand-wrote URLs
over `authFetch`, kept a refresh token in `localStorage`, and left the
generated client mostly unused. It ran on Vite 6, orval 7 and TypeScript 5,
and its tests were `bun test`, which has no DOM, so no component was tested.

The kynos API (#6) serves an OpenAPI 3.1 document committed as
`web/openapi.json`, authenticates the browser with an httpOnly session cookie
(ADR-0004), and reports every failure as RFC 9457 problem+json (ADR-0006).
ADR-0002 proved that document survives orval 8 into a strictly typed
client. The maintainer asked for "a fully modern, nice-to-use, entertaining
interface" and for every part of the stack to be "robust and well-justified".

## Decision

| Concern | Choice |
| --- | --- |
| Runtime, package manager, scripts | Bun |
| Bundler and dev server | Vite 8, `@vitejs/plugin-react` 6 |
| Language | TypeScript 7 (`tsc` 7.0) in strict mode |
| UI | React 19 |
| Routing | TanStack Router, file-based (`src/router/routes`), code-split |
| Server state | TanStack Query 5 |
| API client | orval 8, `client: react-query`, `httpClient: fetch`, one mutator |
| Styling | Tailwind 4 with design tokens as theme variables |
| Accessible primitives | Radix (`radix-ui`): Dialog, DropdownMenu, Popover, Select, Tabs, Toast, Tooltip |
| Class merging | `tailwind-merge` |
| Icons | `lucide-react` |
| Motion | `motion`, behind `prefers-reduced-motion` |
| Drag and drop, charts | `@dnd-kit`, `recharts` 3 (for the list and stats pages) |
| Tests | Vitest 5 + Testing Library + jsdom |
| Lint and format | Biome 2 |
| Offline | A hand-written service worker for the static shell only |

### The API client

orval generates one hook per operation from the committed document; the
generated code is gitignored and rebuilt by `just codegen`, which needs no
Rust. Its configuration (`web/orval.config.ts`) keeps the two choices
ADR-0002 found necessary: the query serializer that drops `null`
(getkono/kynos#992) and the multipart builder for `createImport`'s file part.

It departs from the harness on one point. The harness returns a
`{ data, status, headers }` envelope so a test can assert every status. The
app instead has the mutator (`src/lib/api-client.ts`) resolve to the success
body and **throw** an `ApiProblem` for any other status. TanStack Query only
treats a rejection as an error: with envelopes, every failure would arrive as
"success" data, error boundaries and retries would never fire, and each call
site would re-implement the check. `ApiProblem` keeps the typing the
envelope gave: it is generic over the operation's declared problem union
(orval's `ErrorType<…>` convention), and it exposes `kind` (the error-model
fragment, e.g. `upstream-unavailable`), `retryAfterSeconds` and `retryable`.

The mutator is the only `fetch` in the app. Biome's `noRestrictedGlobals`
flags `fetch` everywhere else, and `src/app.test.ts` also fails on the
retired `authFetch` and on any hand-written `/api/` URL.

### Sessions

The mutator sends `credentials: "include"`. The session is whatever
`GET /sessions/current` says; a 401 there is cached as "signed out", not an
error. Sign-in posts the provider's `code` and `state` to `POST /sessions`
once per code. A 401 from any other call marks the session ended and
re-runs the route guard. No credential is stored by the app.

### Failures and stale data

The query client owns the failure policy. A first load that fails renders an
`ErrorState`; a background refetch or a mutation that fails raises a toast.
Upstream and rate-limit failures are retried once after `Retry-After`;
client errors are never retried. When a response says it came from the
catalog snapshot (`freshness.stale`) or that a provider failed (`degraded`),
the shell shows a banner until no cached response says so.

### Theme and motion

Colours are semantic tokens (`bg-surface`, `text-fg-muted`, …) whose values
switch on `<html data-theme>`. The theme preference lives in the account
(`PATCH /me`), so it follows the user between browsers; the last applied
theme is also kept in `localStorage` only so a reload paints it before the
session loads. `prefers-reduced-motion` turns `motion` animations off
through `<MotionConfig>` and collapses CSS transitions in `global.css`.

### TypeScript 7

orval 8's output and the TanStack Router generator both compile under
`tsc` 7.0.2 with `strict`, `noUncheckedIndexedAccess` and
`verbatimModuleSyntax`; nothing needed relaxing. TypeScript 7 no longer
loads every `@types` package by default, so `tsconfig.json` names
`vite/client` and `node`.

## Alternatives rejected

- **Next.js or Remix (React Router 7 framework mode).** Both earn their keep
  through server rendering, server actions or loaders on a Node server.
  Stacks is a signed-in, self-hosted app whose data lives behind its own
  API: there is no SEO surface, and a second server would add a deploy unit,
  a second auth hop and a second place for the session cookie. A static SPA
  served by nginx next to the API is simpler to run and to reason about.
- **SWR.** Lighter, but no mutation lifecycle, optimistic-update helpers,
  query cancellation or cache events; the stale banner and the global error
  policy are built on TanStack Query's cache. orval generates for both, so
  this is not locked in by the generator.
- **openapi-typescript with openapi-fetch / hey-api.** Weighed in ADR-0002:
  no per-operation TanStack Query hooks (openapi-fetch), or a data-layer
  rewrite with no gap closed (hey-api). orval 8 handles every shape the API
  emits.
- **shadcn/ui.** It is Radix plus Tailwind, copied into the repository. We
  use the same primitives through the single `radix-ui` package and keep our
  own thin wrappers styled with our tokens; copying shadcn's generated
  components would add a CLI, a `components.json` and styles we would
  restyle anyway.
- **cmdk for the palette.** It wraps Radix Dialog with a filtering combobox,
  but its strength is client-side fuzzy matching, and the palette searches
  on the server. The design system needs an accessible Combobox anyway; the
  palette uses it, so there is one combobox implementation.
- **Jest.** Needs a separate transform (Babel or SWC) and ESM configuration
  next to Vite; Vitest reuses the Vite config and is the de facto runner for
  Vite apps. `bun test` stays out because it has no DOM.
- **happy-dom.** Faster, but jsdom's accessibility tree and focus behaviour
  are closer to browsers, which matters for tests that assert ARIA and
  keyboard behaviour.
- **vite-plugin-pwa (Workbox).** Generates a precache manifest and runtime
  caching rules, at the cost of the Workbox build chain. The offline shell
  needs three rules (never touch `/api/`, network-first pages with the
  cached shell as fallback, cache-first fingerprinted assets), which
  `public/sw.js` states directly and `src/lib/service-worker.test.ts`
  exercises.
- **A component library (MUI, Mantine, Chakra).** Each brings its own theme
  system and styling runtime alongside Tailwind; the design would be theirs.

## Consequences

- Every page uses the generated hooks (#9, #10, #11); the browser suite in
  `e2e/` drives them against the composed stack (ADR 0012).
- Every design-system component has a Vitest test, and new ones are expected
  to: `just test-web` runs in CI.
- `recharts` and `@dnd-kit` stay installed for the stats and list pages
  (#10); `recharts` 3 supports React 19.
- When kynos#992 ships, the query serializer can go and the generated
  parameter types tighten (ADR-0002).
