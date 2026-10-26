# PRD: Stacks — Self-Hosted Media Tracking & Availability Service

> **Superseded.** This draft predates the v0.1.0 rewrite and no longer
> describes Stacks: it covers Salvo, JWT and refresh tokens, argon2id, the
> Beam integration, AES-GCM credentials and migrations m001..m016, none of
> which exist. The design is recorded in [`docs/adr/`](docs/adr/), the
> codebase in [`AGENTS.md`](AGENTS.md), and deployment in
> [`docs/DEPLOY.md`](docs/DEPLOY.md). The text below is kept as history:
> it is not maintained, and only the claims a reader could mistake for a
> current feature (a Beam filter, notification events, job types) have
> been removed or corrected.

**Version:** 0.2.0-draft
**Date:** 2026-03-26 (reconciled 2026-05-22)
**Status:** Historical draft; neither the design reference nor the changelog. See [`CHANGELOG.md`](CHANGELOG.md) for what is built.

---

## 1. Overview

Stacks is a self-hosted web service that gives users full ownership of their watch status, ratings, watchlists, and progress data. It delegates all metadata fetching, caching, and representation to the **cameo** library, connects to **Beam** for streaming availability and deep-link playback, and exposes a headless **JSON REST API** (OpenAPI 3.1) that a bundled React frontend and any third-party client can consume.

All user data lives in the operator's own **PostgreSQL** database. If every upstream metadata provider disappeared tomorrow, the local watch state, lists, scores, and history would remain intact. Cameo's cached metadata would go stale but user data is never lost.

### 1.1 Non-Goals

The following are explicitly out of scope:

- Social features (activity feeds, following, shared lists across instances).
- User federation or cross-instance sync.
- Built-in media playback or transcoding.
- Content acquisition, torrent management, or Sonarr/Radarr integration. The optional request pipeline from v1 of this PRD is deferred indefinitely.
- DRM, ads, telemetry, or phoning home.
- Streaming server support beyond Beam in the initial release (architecture must make adding Jellyfin/Plex/Emby straightforward, but they are not built).

---

## 2. Problem Statement

Current self-hosted media tools force users to juggle fragmented services:

**Seerr** handles media requests and library scanning but has no personal watch tracking, no anime database support, and is tightly coupled to the download pipeline. **Reiverr** provides a unified TMDB + Jellyfin browsing UI with plugin-based playback, but lacks AniList/MAL support, multi-source metadata, and granular watch state management. **Trakt, Simkl, and AniList** offer rich tracking but are cloud-only — your data is held hostage, and none connect to your own streaming infrastructure.

Stacks unifies metadata browsing (via cameo), watch state ownership (local PostgreSQL), and streaming availability (Beam) into a single service with a single API.

---

## 3. Target Users

1. **Beam users** who want a tracking layer with deep-link playback integration.
2. **Homelab operators** who want full data ownership for their watch history, scores, and lists — no cloud dependency.
3. **Anime + movie/TV viewers** who are tired of splitting tracking between AniList and a separate service.
4. **Anyone** who wants a self-hosted Trakt/Simkl alternative that runs as a single Rust binary alongside a PostgreSQL instance.

---

## 4. Architecture

### 4.1 High-Level Components

```
┌──────────────────────────────────────────────────────────────┐
│                     Clients                                  │
│  (Reference React UI · Beam · Mobile · 3rd-party API users)  │
└──────────────────────┬───────────────────────────────────────┘
                       │ HTTPS / JSON REST (OpenAPI 3.1)
                       ▼
┌──────────────────────────────────────────────────────────────┐
│                   Stacks Core (Salvo)                        │
│                                                              │
│  ┌──────────┐  ┌──────────────────┐  ┌────────────────────┐ │
│  │ Auth &   │  │  Cameo Bridge    │  │  Streaming         │ │
│  │ User Mgmt│  │  (metadata proxy │  │  Connector         │ │
│  │ (OIDC)   │  │   + ID mapping)  │  │  Registry          │ │
│  └──────────┘  └────────┬─────────┘  └─────────┬──────────┘ │
│                         │                       │            │
│  ┌──────────┐  ┌────────┴─────────┐  ┌─────────┴──────────┐ │
│  │ Watch    │  │  cameo library   │  │  Availability      │ │
│  │ State    │  │  (TMDB, AniList  │  │  Index             │ │
│  │ Engine   │  │   fetch + cache) │  │  (Beam library)    │ │
│  └──────────┘  └──────────────────┘  └────────────────────┘ │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                Scheduler / Job Queue                    │  │
│  │  (library sync · cache warming · import processing)    │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────┬───────────────────────────────────────┘
                       │
          ┌────────────┼──────────────┐
          ▼            ▼              ▼
   ┌────────────┐ ┌──────────┐  ┌──────────┐
   │ PostgreSQL │ │  cameo   │  │  Beam    │
   │  (user     │ │  (TMDB + │  │  Server  │
   │   data)    │ │  AniList │  │          │
   └────────────┘ │  cache)  │  └──────────┘
                  └──────────┘
```

### 4.2 Technology Choices

| Layer | Choice | Rationale |
|---|---|---|
| Language | **Rust** (2024 edition, stable toolchain) | Single-binary deployment, safety, strong async (tokio). |
| HTTP framework | **Salvo** | First-class OpenAPI integration via `#[endpoint]` macros and `salvo-oapi`. Auto-generates OpenAPI 3.1 spec from handler signatures. Built-in Swagger UI, Scalar, RapiDoc, and ReDoc serving. Extracts request parameters, body types, and response schemas directly from Rust types with `ToSchema` / `ToParameters` derives. No separate spec file to maintain. |
| Database | **PostgreSQL (>=18)** via **SeaORM** | Single-process operator footprint (Docker Compose ships it), with the room to scale up if needed. SeaORM's typed entities + migrations replace the original hand-rolled `sqlx` query plan. |
| Metadata | **cameo** (library dependency, git on master) | Handles all upstream API interaction (TMDB, AniList) and the canonical metadata representation. Stacks stores only a self-describing **media id** (`<provider>:<kind>:<numeric>`, opaque to clients) to reference metadata. No FK constraints — the ID is resolved at runtime by cameo. |
| Background jobs | **In-process job queue** backed by a `jobs` table in PostgreSQL | No Redis or external broker. The runner ([`core/src/jobs/runner.rs`](core/src/jobs/runner.rs)) polls, claims, and dispatches with exponential backoff. |
| Reference frontend | **React 19 + Vite + Bun** | Mature ecosystem, fast HMR, wide contributor pool. Focus on motion/animation UX (detailed in §5.9). |
| Containers | **Docker Compose** | Three-file split: `compose.dependencies.yaml` (PostgreSQL 18), `compose.stacks.yaml` (API on `:8095`, web build on `:3000`), `compose.yaml` (`include:`s both). |

### 4.3 Relationship Between Stacks and Cameo

Cameo owns the entire metadata domain: fetching from TMDB and AniList, caching responses, providing a unified data model for movies/shows/anime, and exposing search, discovery, recommendations, and detail queries. Stacks never talks to TMDB or AniList directly.

The interface contract:

```
┌─────────────────────────────────────────────────────────┐
│  What cameo provides (library API)                      │
├─────────────────────────────────────────────────────────┤
│  search(query, filters) → Vec<CameoMedia>               │
│  get(cameo_id) → CameoMedia                             │
│  get_seasons(cameo_id) → Vec<CameoSeason>               │
│  get_episodes(cameo_id, season) → Vec<CameoEpisode>     │
│  discover(category, page) → Vec<CameoMedia>             │
│  recommendations(cameo_id) → Vec<CameoMedia>            │
│  external_ids(cameo_id) → ExternalIds (tmdb, anilist..) │
│  CameoMedia includes: title, overview, poster, backdrop,│
│    genres, cast, crew, ratings, year, media_type, etc.  │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│  What Stacks stores (PostgreSQL)                        │
├─────────────────────────────────────────────────────────┤
│  cameo_media_id (TEXT, self-describing) — links user    │
│    state to a cameo entity. Format:                     │
│    `<provider>:<kind>:<numeric>`. NOT a foreign key.    │
│    Resolved at runtime via cameo.                       │
│  All watch state, scores, lists, history, tags,         │
│    streaming availability mappings.                     │
└─────────────────────────────────────────────────────────┘
```

Stacks's API endpoints for media detail, search, and discovery are **thin proxies** over cameo: they call into cameo, enrich the response with the requesting user's watch state from PostgreSQL, and return the merged result.

### 4.4 Streaming Connector Architecture

The connector layer is designed as a **trait** so adding Jellyfin/Plex/Emby later requires only a new implementation, not a refactor:

```rust
#[async_trait]
pub trait StreamingConnector: Send + Sync {
    /// Human-readable name for UI display
    fn server_type(&self) -> &'static str;

    /// Pull the full library and return matched items.
    /// Each item includes the server's internal ID, quality info,
    /// and a deep-link URL. Matching against cameo is done by
    /// external IDs (tmdb_id, anilist_id, imdb_id).
    async fn sync_library(&self) -> Result<Vec<ConnectorLibraryItem>>;

    /// Generate a direct playback URL for a specific item.
    async fn playback_link(
        &self,
        server_item_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String>;

    /// Process an inbound webhook payload (playback events,
    /// library changes). Returns structured events.
    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &HeaderMap,
    ) -> Result<Vec<ConnectorEvent>>;

    /// Health check — is the server reachable and authenticated?
    async fn health_check(&self) -> Result<ConnectorHealth>;
}

pub struct ConnectorLibraryItem {
    pub cameo_media_id: Option<String>,   // matched via external IDs
    pub external_ids: ExternalIds,         // for matching
    pub server_internal_id: String,
    pub title: String,                     // fallback if unmatched
    pub quality: Option<String>,           // "4K", "1080p", etc.
    pub deep_link: String,
    pub playback_link: String,
}

pub enum ConnectorEvent {
    PlaybackStarted { user_hint: String, server_item_id: String, position_secs: u64 },
    PlaybackStopped { user_hint: String, server_item_id: String, position_secs: u64 },
    PlaybackProgress { user_hint: String, server_item_id: String, position_secs: u64 },
    LibraryItemAdded { item: ConnectorLibraryItem },
    LibraryItemRemoved { server_item_id: String },
}
```

For the initial release, only `BeamConnector` implements this trait. The matching algorithm for library sync:

1. Cameo provides external IDs (tmdb_id, anilist_id, imdb_id) for each media item.
2. Beam's library scan returns its own items with whatever external IDs Beam exposes.
3. Match on exact external ID overlap (any shared ID is a match).
4. Unmatched items are stored with `cameo_media_id = NULL` and flagged for manual resolution.

---

## 5. Features (Detailed)

### 5.1 Search & Discovery (via Cameo)

All metadata queries are delegated to cameo. Stacks enriches responses with user-specific data.

**Unified search:** The frontend search bar calls Stacks's `/api/v1/media/search` endpoint, which calls cameo's search, then for each result looks up the requesting user's watch status from PostgreSQL and attaches it. Results include both locally-known items (user has interacted with them before) and fresh upstream results.

**Discovery feeds:** Trending, popular, top-rated, and upcoming content — sourced from cameo's `discover()` API (which pulls from TMDB and AniList). Presented as scrollable rows on the home page. Each item is annotated with the user's watch status and Beam availability.

**Recommendations:** Per-media-item, fetched via `cameo.recommendations(id)`. Cached by cameo.

**Filtering:** Stacks's API supports structured filters that compose: media type (movie, show, anime_movie, anime_show), genre, year range, watch status, score range, tags. These filters apply both to cameo results (for metadata-level filters) and to local PostgreSQL data (for user-state filters). The API accepts a filter object:

```json
{
  "media_type": ["anime_show"],
  "genre": ["action", "sci-fi"],
  "year_min": 2020,
  "year_max": 2026,
  "status": ["plan_to_watch"],
  "sort": "score_desc"
}
```

### 5.2 Watch State Management

This is the core of Stacks. Every piece of watch state is user-owned, stored in PostgreSQL, and API-accessible.

**Statuses:** `plan_to_watch`, `watching`, `on_hold`, `completed`, `dropped`, `rewatching`. These map cleanly to MAL/AniList conventions and extend naturally to movies and non-anime TV.

**Episode-level tracking:**

- Per-episode watched/unwatched toggle.
- Per-episode resume position (seconds). Updated when Beam sends a playback progress webhook, or set manually via the API.
- Bulk operations: mark season as watched, mark all episodes up to S02E05 as watched. The bulk endpoint accepts an array of episode identifiers and a target state.

**Scoring:**

- Internal storage: 0–100 integer.
- Display preference per user: 5-star (with half-stars), 10-point (with decimals), or 100-point. The conversion is lossless in the 100→lower direction. The user's preference is stored in `user_settings` and the frontend renders accordingly.

**Watch history:**

- Append-only event log: every status change, every playback event, every score change is recorded with a timestamp. Never deleted by normal operations.
- Used to derive statistics: total watch time, episodes per week, genre breakdown, yearly summary, monthly activity heatmap.
- History entries can be manually added (backfill for data imported from other services) or auto-generated by Beam webhook events.

### 5.3 Watchlist & Custom Lists

**Default virtual lists:** Every user gets a `Watchlist` (items with status `plan_to_watch`) and `Favorites` (items with `is_favorite = true`). These are not stored as explicit list entities — they are computed views over watch state.

**Custom lists:** User-created, manually ordered. Each list has a name, optional description, and an ordered list of cameo media IDs. A media item can appear in multiple lists. Reordering is via a `position` float (fractional indexing) so reorder operations are single-row updates.

**Blocklist:** Items the user never wants to see in recommendations or discover feeds. Stored as a set of cameo media IDs. Filtered out at the API response layer.

**Smart lists (saved filters):** A stored filter query (the same JSON filter object from §5.1) that evaluates dynamically. E.g., "Unwatched anime, score > 80, available on Beam." Smart lists have a name and description like custom lists but their contents are computed on each access.

### 5.4 Streaming Availability (Beam)

The availability index answers: "I want to watch X — does my Beam server have it, in what quality, and give me a link."

**Sync lifecycle:**

1. **Initial sync:** On Beam connector setup, Stacks pulls Beam's full library via its API, matches each item against cameo's external ID mappings, and populates `server_library_items`.
2. **Incremental sync:** Webhook-driven. Beam sends library-changed events → Stacks adds/removes entries. Fallback: periodic poll (configurable, default 1 hour).
3. **Playback sync:** When a user watches something on Beam, Beam sends playback webhooks → Stacks updates `episode_watch_status.progress_seconds` and appends to `watch_history`.

**Availability in API responses:** Every media detail and search result includes an `availability` field:

```json
{
  "cameo_media_id": "cmeo_abc123",
  "title": "Steins;Gate",
  "watch_status": "watching",
  "availability": {
    "available": true,
    "servers": [
      {
        "server_id": "srv_beam_1",
        "server_name": "My Beam",
        "quality": "1080p",
        "deep_link": "beam://media/abc123",
        "playback_link": "beam://play/abc123"
      }
    ]
  }
}
```

When no Beam server is configured or the item is unavailable, `availability.available` is `false` and `servers` is empty.

### 5.5 Notifications

**Channels:** Webhooks (generic JSON POST to a user-configured URL). Additionally, Apprise support via a sidecar container for users who want Discord/Telegram/ntfy/Gotify/email/etc. — Stacks sends a JSON payload to the Apprise endpoint and Apprise handles routing.

**Events:**

- New episode aired for a show the user is tracking (status `watching` or `plan_to_watch`).
- Import completed (see §5.7).

**Per-user configuration:** Each user selects which events they subscribe to and which webhook/Apprise channels to route them to. Stored in `notification_subscriptions`.

### 5.6 Multi-User & Permissions

**Authentication is OIDC-only.** Stacks does not maintain local password credentials; bring your own provider (Authelia, Authentik, Keycloak, Dex, …). Users are auto-provisioned on first login, identified by the OIDC `sub` claim (`users.oidc_sub`, unique). The username comes from `preferred_username` when present, else `user_<first-8-of-sub>`.

**Roles:** `admin` (full control: Beam connector config, user management, job management) and `user` (personal watch state, lists, settings, notifications). New users default to `user`; the first admin is promoted manually with an `UPDATE` against the `users` table (see [`docs/DEPLOY.md`](docs/DEPLOY.md)).

**API tokens:** Users can create scoped, expirable API tokens for third-party / non-interactive clients (`stk_<ULID>` plaintext, argon2id-hashed at rest). Scopes: `read`, `write`, `admin`. Plaintext is shown exactly once at creation.

### 5.7 Import

Import is a first-class feature. Users coming from other services need a seamless migration path.

**Supported import sources:**

| Source | Format | What's imported | Mapping strategy |
|---|---|---|---|
| **MyAnimeList** | XML export (from MAL settings → Export) | Anime watch status, score (10-point → scaled to 100), episode count, start/finish dates, rewatch count, tags | Each MAL entry has a `mal_id`. Cameo provides MAL→cameo ID mapping via AniList's cross-reference data. Entries that can't be mapped are logged and skipped with a detailed report. |
| **AniList** | JSON via AniList API (OAuth flow: user authorizes Stacks, we pull their list) | Anime + manga watch/read status, score (configurable scale → normalized to 100), progress, start/finish dates, repeat count, notes, custom lists | AniList IDs map directly to cameo's AniList provider. Near-100% match rate. |
| **TMDB** | Account rated/watchlist/favorites via TMDB API (user provides their TMDB session or v4 auth token) | Rated movies/shows (score), watchlist, favorites | TMDB IDs map directly to cameo's TMDB provider. |
| **Trakt** | JSON export (from Trakt settings or third-party tools) | Watch history, ratings, watchlist, collected items | Trakt entries carry TMDB/IMDB IDs. Mapped via cameo's external ID lookup. |
| **Letterboxd** | CSV export (from Letterboxd settings) | Watched films, ratings (5-star → scaled to 100), watchlist, diary entries with dates | Letterboxd entries carry TMDB IDs in the CSV or are matched by title+year. |
| **Simkl** | JSON export | Watch status, ratings, history | Simkl entries carry TMDB/IMDB/MAL IDs. |
| **Generic CSV** | User-provided CSV with a defined schema | Any field mappable to Stacks's model | Requires column mapping (documented schema). At minimum: title, year, and one external ID. |
| **Stacks JSON** | Full export from another Stacks instance | Everything: watch state, lists, tags, history, settings | Direct restore. Cameo media IDs are stable across instances using the same providers. |

**Import pipeline:**

1. User uploads file (or initiates OAuth flow for AniList/TMDB).
2. Stacks parses the source format into a normalized intermediate representation: `Vec<ImportEntry>` where each entry has external IDs, status, score, progress, dates, etc.
3. For each entry, resolve `cameo_media_id` by querying cameo with the available external IDs. Cameo's `external_ids()` and reverse-lookup provide this.
4. Apply the watch state to the user's local data. **Conflict resolution:** if the user already has a status for a media item, the import uses a configurable strategy: `skip` (keep existing), `overwrite` (replace with imported), or `merge` (keep the more-progressed state — e.g., if local says episode 5 and import says episode 8, keep 8).
5. Generate an import report: total entries, successfully imported, skipped (already exists), failed (unmapped), and a downloadable list of failed entries for manual resolution.
6. Import is **idempotent** — re-importing the same file with `skip` conflict strategy produces no changes.

**Import is a background job.** Large MAL/AniList libraries (1000+ entries) are processed async. The user sees a progress indicator and gets a notification on completion.

### 5.8 Export

- **Stacks JSON:** Full dump of all user data (watch state, lists, tags, history, settings, notification preferences). This is the "if everything burns, I keep my data" escape hatch. Also serves as the backup-and-restore format.
- **CSV:** Flat export of watch state: cameo_media_id, title, media_type, status, score, episode_progress, started_at, completed_at, tags.
- **MAL-compatible XML:** For users who want to import back into MAL. Only includes anime entries with `mal_id` mappings.

Export endpoints: `GET /api/v1/users/me/export?format=json|csv|mal_xml`

### 5.9 Reference Frontend: UX & Animation Design

The frontend is a React 19 SPA built with Vite, served as static assets from the Stacks binary (or a separate container if preferred). It communicates exclusively via the OpenAPI-specified REST API.

**Design philosophy:** The UI should feel like a high-quality native app, not a typical admin dashboard. Visual density is high (media posters are the primary content), interactions are fast and tactile, and transitions reinforce spatial relationships.

**Core layout:**

- **Sidebar navigation** (collapsible): Home/Discover, Search, Library (user's tracked items), Lists, Stats, Settings. The sidebar is persistent on desktop, a bottom tab bar on mobile.
- **Home page:** Horizontal scrollable rows — "Continue Watching" (items with in-progress episodes), "Recently Added to Beam", "Trending", "Popular", "Top Rated". Each row is a horizontally scrollable list of poster cards.
- **Library page:** The user's tracked items, filterable by status tabs (Watching, Plan to Watch, Completed, On Hold, Dropped, All). Grid of poster cards with sort controls (title, score, last updated, date added). Status tabs use an animated underline indicator that slides between tabs.
- **Media detail page:** Full-bleed backdrop image at top with gradient fade to content. Below: title, year, genres, user's score (editable inline), status selector (dropdown), synopsis, cast/crew, seasons/episodes (for shows), and availability on Beam with a prominent "Play on Beam" button. For shows: episode list with per-episode watched checkboxes and a "mark all up to here" gesture.

**Animation & transition expectations:**

| Interaction | Animation | Duration | Easing |
|---|---|---|---|
| **Page navigation** | Crossfade with subtle scale (outgoing page scales to 0.98 and fades, incoming fades in at 1.0) | 200ms | ease-out |
| **Poster card hover** | Scale to 1.05, elevate shadow, show title overlay | 150ms | ease-out |
| **Poster card click → detail page** | Shared element transition: the poster card expands and morphs into the detail page backdrop/poster position. Use the View Transitions API where supported, fallback to CSS transform + opacity. | 300ms | cubic-bezier(0.4, 0, 0.2, 1) |
| **Status tab switching** | Underline indicator slides to the new tab. Content crossfades. | 200ms | ease-in-out |
| **Dropdown/modal open** | Scale from 0.95 + fade in, with a subtle spring overshoot | 250ms | spring(1, 80, 10) |
| **Dropdown/modal close** | Scale to 0.95 + fade out | 150ms | ease-in |
| **Toast notifications** | Slide in from top-right, auto-dismiss with a shrinking progress bar | enter: 300ms, exit: 200ms | ease-out |
| **Score change** | Number morphs (old value fades up and out, new value fades up and in) | 200ms | ease-out |
| **Episode checkbox toggle** | Checkbox fills with a brief scale pulse (1.0 → 1.2 → 1.0). The episode row briefly highlights with a background color sweep left-to-right. | 300ms | ease-out |
| **List reorder (drag-and-drop)** | Dragged item lifts with shadow and scale 1.03. Other items slide smoothly to make space. Drop snaps into position. | 200ms per shift | ease-out |
| **Horizontal row scroll** | Momentum-based with snap points on poster boundaries. Leading/trailing peek of next items. | physics-based | deceleration |
| **Pull-to-refresh (mobile)** | Overscroll reveals a spinner that scales in. Release triggers refresh with a bounce. | 300ms | spring |
| **Skeleton loading** | Shimmer animation (gradient sweep left-to-right) on placeholder rectangles matching content layout. | 1.5s loop | linear |
| **Availability badge** | "Available on Beam" badge pulses once on first render (scale 1.0 → 1.1 → 1.0) to draw attention. | 400ms | ease-in-out |
| **Search** | Results stream in with staggered fade-in (each card delays 30ms after the previous). Search field expands on focus with a width transition. | 200ms + 30ms stagger | ease-out |
| **Empty states** | Illustrated SVG with a gentle floating animation (translateY oscillation ±4px). | 3s loop | ease-in-out |

**Animation library:** Use **Framer Motion** (now `motion`) for React. It handles layout animations, shared element transitions, gesture-based interactions (drag-and-drop), and spring physics natively. Avoid CSS-only animations for anything interactive — Framer Motion's declarative API (`<motion.div>`, `AnimatePresence`, `layoutId`) is significantly easier to maintain and produces more consistent results.

**Responsiveness:** Three breakpoints — mobile (<768px), tablet (768–1024px), desktop (>1024px). Poster grid columns adjust: 2 → 3 → 5+. The sidebar collapses to a bottom bar on mobile. All touch targets are ≥44px.

**Dark/light mode:** System-preference default with manual override. Colors are defined as CSS custom properties. The media-heavy UI works best in dark mode (less glare around poster images), so dark mode is the default when no system preference is detected.

**Key frontend dependencies:**

| Package | Purpose |
|---|---|
| `react` + `react-dom` (19.x) | Core |
| `@tanstack/react-query` | Server state management, caching, optimistic updates |
| `@tanstack/react-router` | File-based routing with type-safe params |
| `motion` (Framer Motion) | Animation and gesture handling |
| `tailwindcss` | Utility-first styling |
| `orval` or `openapi-typescript` | Auto-generate typed API client from OpenAPI spec |
| `react-virtuoso` | Virtualized lists for large libraries |
| `@dnd-kit/core` | Accessible drag-and-drop for list reordering |

---

## 6. Database Schema

Since cameo owns metadata, Stacks's schema is focused entirely on user data and connector state. The `cameo_media_id` column appears throughout but has **no FK constraint** — it is a self-describing `<provider>:<kind>:<numeric>` string resolved at runtime via cameo.

The authoritative schema is the **SeaORM migration set** in [`core/migration/src/`](core/migration/src/) (`m001`..`m016`); the runtime entities are in [`core/src/entity/`](core/src/entity/). What follows is a logical summary; consult the migrations for column types, defaults, and constraints, and the entity modules for relations.

### 6.1 User Tables

- **`users`** — Core account. Identified by `oidc_sub` (unique) — Stacks is OIDC-only, there is no `password_hash`. Carries `username` (unique), optional `email`, `role` (`'user'` or `'admin'`, default `'user'`), and a JSON `settings` blob.
- **`api_tokens`** — User-issued bearer tokens for non-interactive clients. Argon2id-hashed (`stk_…` plaintext shown once), with `scopes` JSON (`read` / `write` / `admin`) and optional `expires_at`.
- **`refresh_tokens`** — Session refresh tokens; stored as SHA-256 hashes, rotated on each refresh.

### 6.2 Watch State Tables

- **`watch_status`** — Per-user, per-media canonical watch state. Status is one of `plan_to_watch`, `watching`, `on_hold`, `completed`, `dropped`, `rewatching`. Score is `0..=100`. Composite primary key `(user_id, cameo_media_id)`.
- **`episode_watch_status`** — Per-user, per-episode watched / progress / `watched_at`. `cameo_episode_id` is the self-describing episode id (`<provider>:tv:<show>:<season>:<episode>`).
- **`watch_history`** — Append-only event log feeding the stats endpoints. Source values include `manual`, `beam_webhook`, `import`.

### 6.3 List & Tag Tables

- **`user_lists`** — Custom and smart lists. Smart lists store a `filter_query` JSON; manual lists store ordered membership in `user_list_items`.
- **`user_list_items`** — Membership rows for manual lists with `position` (REAL) for fractional-index reordering.
- **`user_tags`** + **`media_tags`** — Per-user tag definitions and their assignments to media items.
- **`blocklist`** — `cameo_media_id`s suppressed from recommendations and discover feeds.

### 6.4 Streaming Connector Tables

- **`streaming_servers`** — Registered connector instances. Today `server_type = 'beam'` is the only supported value. `api_key_enc` is AES-256-GCM encrypted with a key derived from `WW_SECRET_KEY`. Optional `webhook_secret` is stored plaintext (it is itself a verification secret).
- **`server_library_items`** — Per-server cached library entries. `cameo_media_id` is nullable so unmatched entries are retained for later resolution. Unique constraint on `(server_id, server_internal_id)`.

### 6.5 Notification & Job Tables

- **`notification_channels`** — Per-user channels (`webhook`, `apprise`) with a free-form `config` JSON.
- **`notification_subscriptions`** — Per-user event subscriptions (`new_episode`, `import_completed`).
- **`jobs`** — Background queue. Job types today are `import`, `notification`, `metadata_refresh`. Statuses are `pending`, `running`, `completed`, `failed`. The runner ([`core/src/jobs/runner.rs`](core/src/jobs/runner.rs)) claims pending rows whose `run_at` has elapsed, attempts the job up to `max_attempts`, and reschedules with exponential backoff (30s, 60s, 120s, …, capped at 1h) on transient failure.

---

## 7. API Design

### 7.1 Philosophy

- **JSON REST** over HTTP/1.1 and HTTP/2.
- **OpenAPI 3.1 spec** auto-generated from Salvo `#[endpoint]` annotations and `ToSchema`/`ToParameters` derives. Served at `/api/docs` (Swagger UI) and `/api/openapi.json`. Zero manual spec maintenance.
- **Versioned:** All endpoints under `/api/v1/`. Breaking changes increment the version.
- **Auth:** Bearer token (JWT, 15-minute lifetime, configurable) + refresh token (opaque, long-lived, stored hashed in `refresh_tokens`). API tokens as an alternative for non-interactive clients.
- **Pagination:** Cursor-based (`?cursor=<ulid>&limit=50`). No offset-based pagination.
- **Filtering:** Query parameter filter objects for listing endpoints.
- **Bulk operations:** POST endpoints that accept arrays for batch watch-state updates.
- **Error responses:** Consistent shape: `{ "error": { "code": "NOT_FOUND", "message": "...", "details": {...} } }`.

### 7.2 Salvo OpenAPI Integration

Every handler uses `#[endpoint]` and request/response types derive `ToSchema`:

```rust
use salvo::prelude::*;
use salvo::oapi::extract::*;

#[derive(ToSchema, Serialize)]
struct MediaSearchResult {
    items: Vec<MediaItem>,
    next_cursor: Option<String>,
}

#[derive(ToParameters, Deserialize)]
struct SearchParams {
    /// Search query string
    #[salvo(parameter(parameter_in = Query))]
    q: String,
    /// Maximum results to return
    #[salvo(parameter(parameter_in = Query))]
    limit: Option<u32>,
    /// Cursor for pagination
    #[salvo(parameter(parameter_in = Query))]
    cursor: Option<String>,
}

#[endpoint(
    tags("Media"),
    summary = "Search media",
    description = "Search across all media types via cameo, enriched with user watch state"
)]
async fn search_media(
    params: SearchParams,
    depot: &mut Depot,  // extracts auth context
) -> Json<MediaSearchResult> {
    // ...
}
```

Salvo generates the OpenAPI spec entry for this endpoint automatically: path, parameters, request/response schemas, and documentation from doc comments.

### 7.3 Resource Overview

| Resource | Endpoints |
|---|---|
| **Auth** | `POST /auth/login` (begin OIDC), `GET /auth/callback`, `POST /auth/refresh`, `POST /auth/logout`, `GET /auth/tokens`, `POST /auth/tokens`, `DELETE /auth/tokens/:id` |
| **Users** | `GET /users/me`, `PATCH /users/me`, `PATCH /users/me/settings`, `GET /users` (admin), `DELETE /users/:id` (admin). No `POST /users` — accounts are auto-provisioned by OIDC on first login. |
| **Media** | `GET /media/search`, `GET /media/:cameo_id`, `GET /media/:cameo_id/seasons`, `GET /media/:cameo_id/seasons/:num/episodes`, `GET /media/:cameo_id/recommendations` |
| **Watch State** | `GET /users/me/media/:cameo_id/status`, `PUT /users/me/media/:cameo_id/status`, `DELETE /users/me/media/:cameo_id/status`, `GET /users/me/episodes/:cameo_episode_id/status`, `PUT /users/me/episodes/:cameo_episode_id/status`, `POST /users/me/media/bulk-status` |
| **Watch History** | `GET /users/me/history` (paginated, filterable), `POST /users/me/history` (manual entry / backfill) |
| **Lists** | `GET /users/me/lists`, `POST /users/me/lists`, `GET /users/me/lists/:id`, `PUT /users/me/lists/:id`, `DELETE /users/me/lists/:id`, `POST /users/me/lists/:id/items`, `DELETE /users/me/lists/:id/items/:cameo_id`, `PUT /users/me/lists/:id/items/reorder` |
| **Tags** | `GET /users/me/tags`, `POST /users/me/tags`, `DELETE /users/me/tags/:id`, `PUT /users/me/media/:cameo_id/tags` |
| **Blocklist** | `GET /users/me/blocklist`, `POST /users/me/blocklist`, `DELETE /users/me/blocklist/:cameo_id` |
| **Discover** | `GET /discover/trending`, `GET /discover/popular`, `GET /discover/upcoming`, `GET /discover/top-rated` (all accept `?media_type=` filter) |
| **Availability** | `GET /media/:cameo_id/availability`, `GET /servers` (list connected streaming servers), `GET /servers/:id/status` |
| **Notifications** | `GET /users/me/notification-channels`, `POST /users/me/notification-channels`, `PUT /users/me/notification-channels/:id`, `DELETE /users/me/notification-channels/:id`, `GET /users/me/notification-preferences`, `PUT /users/me/notification-preferences` |
| **Import/Export** | `POST /users/me/import` (JSON body with base64-encoded source; `source` in `mal_xml`, `mal_json`, `tmdb_json`), `GET /users/me/import/:job_id` (status), `GET /users/me/export?format=json\|csv` |
| **Admin** | `GET /admin/servers`, `POST /admin/servers`, `PUT /admin/servers/:id`, `DELETE /admin/servers/:id`, `POST /admin/servers/:id/sync` (trigger immediate), `GET /admin/jobs`, `POST /admin/jobs/:id/retry` |
| **Stats** | `GET /users/me/stats` (total watch time, genre breakdown, per-year counts, monthly heatmap, media type distribution) |
| **Webhooks** | `POST /webhooks/beam/:server_id` (receives Beam event payloads, validated by webhook secret) |
| **Health** | `GET /health`, `GET /health/ready` |

---

## 8. Deployment

### 8.1 Docker Compose

The repo ships a three-file Compose split:

- [`compose.dependencies.yaml`](compose.dependencies.yaml) — PostgreSQL 18.
- [`compose.stacks.yaml`](compose.stacks.yaml) — the `stacks-api` container (`:8095`) and the static `stacks-web` build (`:3000`).
- [`compose.yaml`](compose.yaml) — a one-liner that `include:`s both.

```bash
export WW_TMDB_API_KEY=...
export WW_OIDC_ISSUER=https://auth.example.com
export WW_OIDC_CLIENT_ID=stacks
export WW_OIDC_CLIENT_SECRET=...
export WW_OIDC_REDIRECT_URI=https://stacks.example.com/auth/callback

docker compose up -d
```

The API container is stateless and mounts no volume. Migrations run automatically on startup. See [`docs/DEPLOY.md`](docs/DEPLOY.md) for the full production guide (reverse proxy, OIDC provider tips, Beam server registration, backup, observability).

### 8.2 Configuration

All configuration comes from environment variables, optionally layered over a `config.toml` (path from `WW_CONFIG_FILE`, default `config.toml`). Env vars take precedence. The full table — including required vs. optional vs. defaulted — lives in the [README](README.md#configuration); the authoritative source is [`core/src/config/env.rs`](core/src/config/env.rs).

Required: `WW_DATABASE_URL`, `WW_TMDB_API_KEY`, `WW_OIDC_ISSUER`, `WW_OIDC_CLIENT_ID`, `WW_OIDC_CLIENT_SECRET`, `WW_OIDC_REDIRECT_URI`.

### 8.3 Backup & Restore

- **PostgreSQL.** `pg_dump` against the `stacks` database is the canonical backup. This is where every piece of user data lives (watch state, lists, history, encrypted streaming-server credentials, queued jobs).
- **User-facing export.** `GET /api/v1/users/me/export?format=json|csv` returns a portable per-user dump of watch state.
- **Import-based restore.** `POST /api/v1/users/me/import` accepts the supported source formats (currently `mal_xml`, `mal_json`, `tmdb_json`). A full Stacks-to-Stacks JSON restore round-trip is not yet implemented; for full restores use the PostgreSQL backup.

---

## 9. Observability

- **Structured logging:** JSON to stdout via `tracing` + `tracing-subscriber`. Filter at the source with `WW_LOG_LEVEL` (any `EnvFilter` string). Each request emits a log with method, path, status, and elapsed time.
- **Metrics:** Prometheus text format at `/metrics` (unauthenticated — restrict at the reverse proxy). Built-in: `http_requests_total{method, path, status}` (counter), `http_request_duration_seconds{method, path}` (histogram). Domain modules add metrics as needed.
- **Health checks:** `/health` (liveness — process is up) and `/health/ready` (readiness — database reachable).

---

## 10. Security

- API keys for Beam servers are encrypted at rest (AES-256-GCM, key derived from `WW_SECRET_KEY` via SHA-256 with a domain-separated prefix — see [`core/src/crypto.rs`](core/src/crypto.rs)).
- Auth is OIDC-only — there are no local password credentials to leak.
- JWT access tokens are short-lived (15 min default, `WW_ACCESS_TOKEN_TTL_SECS`). Refresh tokens stored as SHA-256 hashes and rotated on every refresh.
- API tokens stored as argon2id hashes (`stk_<ULID>` plaintext shown once at creation).
- CORS is allowlist-based (`WW_CORS_ALLOWED_ORIGINS`). A bare `*` is accepted but logs a warning. Default falls back to localhost dev ports.
- The public auth router is per-IP rate-limited to 30 req/min (`api/src/middleware/rate_limit.rs`), honouring `X-Forwarded-For` behind a trusted proxy.
- Security headers are emitted on every response: `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Content-Security-Policy: default-src 'self'`, `Referrer-Policy: strict-origin-when-cross-origin`.
- Beam webhooks are authenticated by HMAC-SHA256 signature (`X-Beam-Signature` or `X-Hub-Signature-256`, optionally prefixed `sha256=`).

---

## 11. Phased Delivery

### Phase 1 — Core (MVP) — done

- Salvo server with auto-generated OpenAPI 3.1, PostgreSQL via SeaORM, migrations on startup.
- Cameo integration: search, detail, seasons / episodes, recommendations, discover feeds (trending / popular / top-rated for both movies and TV).
- Auth via OIDC; JWT access + refresh tokens; opaque API tokens with `read` / `write` / `admin` scopes.
- Watch state: status, score, episode tracking, append-only history, bulk updates.
- Watchlist, favourites, custom + smart lists, tags, blocklist.
- Beam connector: library sync (matched by external provider ids), availability, deep / playback links, HMAC-verified webhooks.
- Import: MyAnimeList XML, MAL JSON, TMDB JSON via background job.
- Export: JSON, CSV.
- Background job runner (PostgreSQL-backed, exponential backoff, admin retry).
- Periodic Beam sync scheduler.
- Notifications: webhook + Apprise channels with per-event subscriptions.
- Stats: yearly counts, monthly heatmap, genre breakdown, estimated watch-time.
- Prometheus `/metrics`, JSON logs, liveness + readiness.
- Hardening middleware: CORS allowlist, rate limiter, security headers, encrypted streaming-server credentials.
- Reference frontend: home, search, library, media detail, lists, stats, settings, admin — wired to the live API via an orval-generated typed client.
- Docker Compose deployment (API + web + PostgreSQL).

### Phase 2 — Reach

- Additional import sources: AniList (OAuth flow), Trakt JSON, Letterboxd CSV, Simkl, generic CSV.
- Export: MAL-compatible XML; full Stacks-to-Stacks JSON round-trip.
- Unmatched-item resolution UI (admin) for Beam library entries cameo couldn't match.
- Full frontend animation pass per §5.9 spec.
- Resolve-by-external-id for non-TMDB providers in the import pipeline once cameo exposes a find-by-external-id endpoint.

### Phase 3 — Extensibility

- Additional streaming connectors (Jellyfin, Plex, Emby) implementing the `StreamingConnector` trait.
- Per-Stacks-user mapping of Beam webhook playback events so they write to watch history.
- Import conflict resolution UI (preview diffs before applying).
- Helm chart for Kubernetes.

---

## 12. Open Questions

1. **Beam integration protocol:** Beam's library API shape, external ID exposure, webhook format, and deep-link URL scheme need to be co-designed. The `StreamingConnector` trait defines what Stacks needs; Beam needs to satisfy it.
2. **Beam-user ↔ Stacks-user mapping:** Inbound playback webhooks carry a Beam-local user id. Stacks currently logs these events for observability but does not write them to watch history because no mapping exists. Phase 3 adds a per-server `(beam_user_id, stacks_user_id)` table populated either by the admin or by an OIDC claim shared between Stacks and Beam.
3. **Cameo external-id resolution:** The import pipeline can only resolve TMDB ids today (`core/src/import_export/import/pipeline.rs::resolve_cameo_id`). MAL / AniList / IMDB / TVDB resolution waits on a find-by-external-id endpoint in cameo.
4. **Cameo library vs. service boundary:** Cameo is currently linked into the binary as a library. If cameo grows heavy or needs independent scaling, it could become a sidecar service — but for now, in-process is simpler and eliminates network overhead.
5. **AniList rate limits for bulk import:** AniList's API allows 90 requests per minute. A user with a 2000-entry anime list would need ~22 requests (90 per page). The import pipeline should respect rate limits with backoff and report progress.
6. **Fractional indexing for list reorder:** Using `REAL` for position avoids reindexing the whole list on every drag-drop, but after many insertions the precision can degrade. Periodic re-normalisation should run when the gap between adjacent items drops below a threshold.
7. **Cameo response cache:** Currently disabled because cameo's optional `cache` feature pulls `rusqlite` -> `libsqlite3-sys`, colliding with the copy `sqlx` already links via SeaORM. Re-enabling it requires resolving that linkage (e.g. via the workspace `links` key) or replacing the cache backend.
