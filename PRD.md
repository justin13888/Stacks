# PRD: Stacks — Self-Hosted Media Tracking & Availability Service

**Version:** 0.2.0-draft
**Date:** 2026-03-26
**Status:** RFC

---

## 1. Overview

Stacks is a self-hosted web service that gives users full ownership of their watch status, ratings, watchlists, and progress data. It delegates all metadata fetching, caching, and representation to the **cameo** library, connects to **Beam** for streaming availability and deep-link playback, and exposes a headless **JSON REST API** (OpenAPI 3.1) that a bundled React frontend and any third-party client can consume.

All user data lives in the operator's own **SQLite** database. If every upstream metadata provider disappeared tomorrow, the local watch state, lists, scores, and history would remain intact. Cameo's cached metadata would go stale but user data is never lost.

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

Stacks unifies metadata browsing (via cameo), watch state ownership (local SQLite), and streaming availability (Beam) into a single service with a single API.

---

## 3. Target Users

1. **Beam users** who want a tracking layer with deep-link playback integration.
2. **Homelab operators** who want full data ownership for their watch history, scores, and lists — no cloud dependency.
3. **Anime + movie/TV viewers** who are tired of splitting tracking between AniList and a separate service.
4. **Anyone** who wants a self-hosted Trakt/Simkl alternative that runs as a single binary and a single SQLite file.

---

## 4. Architecture

### 4.1 High-Level Components

```
┌──────────────────────────────────────────────────────────────┐
│                     Clients                                  │
│  (Reference React UI · Beam · Mobile · CLI · 3rd-party)      │
└──────────────────────┬───────────────────────────────────────┘
                       │ HTTPS / JSON REST (OpenAPI 3.1)
                       ▼
┌──────────────────────────────────────────────────────────────┐
│                   Stacks Core (Salvo)                     │
│                                                              │
│  ┌──────────┐  ┌──────────────────┐  ┌────────────────────┐ │
│  │ Auth &   │  │  Cameo Bridge    │  │  Streaming         │ │
│  │ User Mgmt│  │  (metadata proxy │  │  Connector         │ │
│  │          │  │   + ID mapping)  │  │  Registry          │ │
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
   ┌──────────┐  ┌──────────┐  ┌──────────┐
   │  SQLite  │  │  cameo   │  │  Beam    │
   │  (user   │  │  (TMDB + │  │  Server  │
   │   data)  │  │  AniList │  │          │
   └──────────┘  │  cache)  │  └──────────┘
                 └──────────┘
```

### 4.2 Technology Choices

| Layer | Choice | Rationale |
|---|---|---|
| Language | **Rust** | Single-binary deployment, safety, strong async (tokio). |
| HTTP framework | **Salvo** | First-class OpenAPI integration via `#[endpoint]` macros and `salvo-oapi`. Auto-generates OpenAPI 3.1 spec from handler signatures. Built-in Swagger UI, Scalar, RapiDoc, and ReDoc serving. Extracts request parameters, body types, and response schemas directly from Rust types with `ToSchema` / `ToParameters` derives. No separate spec file to maintain. |
| Database | **SQLite only** | Zero-config, single-file backup, no external process. Uses `sqlx` with compile-time checked queries. WAL mode for concurrent reads. |
| Metadata | **cameo** (library dependency) | Handles all upstream API interaction (TMDB, AniList), caching, and the canonical metadata representation. Stacks stores only a **cameo media ID** (opaque string) to reference metadata. No FK constraints — the ID is resolved at runtime by cameo. |
| Background jobs | **In-process job queue** backed by a `jobs` table in SQLite | No Redis or external broker. Advisory locking via SQLite transactions. |
| Reference frontend | **React 19 + Vite** | Mature ecosystem, fast HMR, wide contributor pool. Focus on motion/animation UX (detailed in §5.9). |
| Container | **Single Docker image** (multi-stage build) | Rust binary + pre-built React static assets + SQLite. Everything in one image, one port. |

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
│  What Stacks stores (SQLite)                        │
├─────────────────────────────────────────────────────────┤
│  cameo_media_id (TEXT, opaque) — links user state to    │
│    a cameo entity. NOT a foreign key. Resolved at       │
│    runtime via cameo.get(id).                           │
│  All watch state, scores, lists, history, tags,         │
│    streaming availability mappings.                     │
└─────────────────────────────────────────────────────────┘
```

Stacks's API endpoints for media detail, search, and discovery are **thin proxies** over cameo: they call into cameo, enrich the response with the requesting user's watch state from SQLite, and return the merged result.

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

**Unified search:** The frontend search bar calls Stacks's `/api/v1/media/search` endpoint, which calls `cameo.search()`, then for each result looks up the requesting user's watch status from SQLite and attaches it. Results include both locally-known items (user has interacted with them before) and fresh upstream results.

**Discovery feeds:** Trending, popular, top-rated, and upcoming content — sourced from cameo's `discover()` API (which pulls from TMDB and AniList). Presented as scrollable rows on the home page. Each item is annotated with the user's watch status and Beam availability.

**Recommendations:** Per-media-item, fetched via `cameo.recommendations(id)`. Cached by cameo.

**Filtering:** Stacks's API supports structured filters that compose: media type (movie, show, anime_movie, anime_show), genre, year range, watch status, score range, Beam availability, tags. These filters apply both to cameo results (for metadata-level filters) and to local SQLite data (for user-state filters). The API accepts a filter object:

```json
{
  "media_type": ["anime_show"],
  "genre": ["action", "sci-fi"],
  "year_min": 2020,
  "year_max": 2026,
  "status": ["plan_to_watch"],
  "available_on_beam": true,
  "sort": "score_desc"
}
```

### 5.2 Watch State Management

This is the core of Stacks. Every piece of watch state is user-owned, stored in SQLite, and API-accessible.

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
- Media became available on Beam (a tracked item appeared in a library sync).
- Upcoming release reminder (configurable lead time: 1 day, 3 days, 1 week).
- Import completed (see §5.7).

**Per-user configuration:** Each user selects which events they subscribe to and which webhook/Apprise channels to route them to. Stored in `user_notification_preferences`.

### 5.6 Multi-User & Permissions

**Local accounts:** Username + bcrypt-hashed password. No external auth dependency by default.

**Optional SSO:** OIDC/OAuth2 relying party support (Authelia, Authentik, Keycloak). Configured via environment variables. When enabled, local password auth can be disabled.

**Roles:** `admin` (full control: Beam connector config, user management, import/export, job management) and `user` (personal watch state, lists, settings, notifications).

**API tokens:** Users can create scoped, expirable API tokens for third-party client access. Token is displayed once at creation, stored as a bcrypt hash. Scopes: `read`, `write`, `admin`.

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

Since cameo owns metadata, Stacks's schema is focused entirely on user data and connector state. The `cameo_media_id` column appears throughout but has **no FK constraint** — it is an opaque string resolved at runtime.

### 6.1 User Tables

```sql
-- Core user account
CREATE TABLE users (
    id             TEXT PRIMARY KEY,  -- ULID
    username       TEXT NOT NULL UNIQUE,
    email          TEXT,
    password_hash  TEXT,              -- bcrypt, NULL if SSO-only
    role           TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('admin', 'user')),
    settings       TEXT NOT NULL DEFAULT '{}',  -- JSON: score_format, theme, language, etc.
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- API tokens for non-interactive clients
CREATE TABLE api_tokens (
    id             TEXT PRIMARY KEY,  -- ULID
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash     TEXT NOT NULL,     -- bcrypt of the token
    label          TEXT NOT NULL,
    scopes         TEXT NOT NULL DEFAULT '["read"]',  -- JSON array: read, write, admin
    expires_at     TEXT,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Refresh tokens for session auth
CREATE TABLE refresh_tokens (
    id             TEXT PRIMARY KEY,
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash     TEXT NOT NULL,
    expires_at     TEXT NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
```

### 6.2 Watch State Tables

```sql
-- Per-user watch status for a media item
CREATE TABLE watch_status (
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cameo_media_id  TEXT NOT NULL,  -- opaque, no FK
    status          TEXT NOT NULL CHECK (status IN (
                        'plan_to_watch', 'watching', 'on_hold',
                        'completed', 'dropped', 'rewatching'
                    )),
    score           INTEGER CHECK (score IS NULL OR (score >= 0 AND score <= 100)),
    score_date      TEXT,
    started_at      TEXT,
    completed_at    TEXT,
    repeat_count    INTEGER NOT NULL DEFAULT 0,
    notes           TEXT,
    is_favorite     INTEGER NOT NULL DEFAULT 0,
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, cameo_media_id)
);
CREATE INDEX idx_watch_status_user_status ON watch_status(user_id, status);
CREATE INDEX idx_watch_status_user_favorite ON watch_status(user_id, is_favorite) WHERE is_favorite = 1;

-- Per-user per-episode tracking
-- cameo_episode_id is also opaque (cameo's episode identifier)
CREATE TABLE episode_watch_status (
    user_id           TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cameo_media_id    TEXT NOT NULL,   -- parent show/anime
    cameo_episode_id  TEXT NOT NULL,   -- opaque episode identifier from cameo
    season_number     INTEGER,         -- denormalized for query convenience
    episode_number    INTEGER,         -- denormalized for query convenience
    watched           INTEGER NOT NULL DEFAULT 0,
    watched_at        TEXT,
    progress_seconds  INTEGER,         -- resume position
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, cameo_episode_id)
);
CREATE INDEX idx_ep_status_media ON episode_watch_status(user_id, cameo_media_id);

-- Append-only watch history log
CREATE TABLE watch_history (
    id              TEXT PRIMARY KEY,  -- ULID
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cameo_media_id  TEXT NOT NULL,
    cameo_episode_id TEXT,             -- NULL for movies
    event           TEXT NOT NULL CHECK (event IN (
                        'started', 'paused', 'resumed', 'completed',
                        'status_changed', 'score_changed'
                    )),
    position_seconds INTEGER,
    metadata        TEXT,              -- JSON: old_status, new_status, old_score, new_score, etc.
    source          TEXT NOT NULL DEFAULT 'manual' CHECK (source IN (
                        'manual', 'beam_webhook', 'import'
                    )),
    timestamp       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX idx_history_user_time ON watch_history(user_id, timestamp DESC);
CREATE INDEX idx_history_user_media ON watch_history(user_id, cameo_media_id);
```

### 6.3 List Tables

```sql
-- User-created custom lists
CREATE TABLE user_lists (
    id           TEXT PRIMARY KEY,  -- ULID
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    description  TEXT,
    is_smart     INTEGER NOT NULL DEFAULT 0,
    filter_query TEXT,              -- JSON filter object for smart lists, NULL for manual lists
    sort_order   TEXT DEFAULT 'position',
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Items in manual (non-smart) lists
CREATE TABLE user_list_items (
    list_id         TEXT NOT NULL REFERENCES user_lists(id) ON DELETE CASCADE,
    cameo_media_id  TEXT NOT NULL,
    position        REAL NOT NULL,     -- fractional indexing for O(1) reorder
    added_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    notes           TEXT,
    PRIMARY KEY (list_id, cameo_media_id)
);

-- User-defined tags
CREATE TABLE user_tags (
    id       TEXT PRIMARY KEY,  -- ULID
    user_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    color    TEXT,              -- hex color, e.g., "#FF6B6B"
    UNIQUE (user_id, name)
);

-- Tag assignments
CREATE TABLE media_tags (
    user_id         TEXT NOT NULL,
    cameo_media_id  TEXT NOT NULL,
    tag_id          TEXT NOT NULL REFERENCES user_tags(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, cameo_media_id, tag_id)
);

-- Blocklist
CREATE TABLE blocklist (
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cameo_media_id  TEXT NOT NULL,
    added_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, cameo_media_id)
);
```

### 6.4 Streaming Connector Tables

```sql
-- Registered streaming servers (Beam only for now)
CREATE TABLE streaming_servers (
    id              TEXT PRIMARY KEY,  -- ULID
    name            TEXT NOT NULL,
    server_type     TEXT NOT NULL DEFAULT 'beam' CHECK (server_type IN ('beam')),
    base_url        TEXT NOT NULL,
    api_key_enc     BLOB,             -- AES-256-GCM encrypted
    user_id         TEXT REFERENCES users(id) ON DELETE SET NULL,
                                      -- NULL = admin-global, set = per-user
    webhook_secret  TEXT,
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_synced_at  TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Items found on connected streaming servers
CREATE TABLE server_library_items (
    id                TEXT PRIMARY KEY,  -- ULID
    server_id         TEXT NOT NULL REFERENCES streaming_servers(id) ON DELETE CASCADE,
    cameo_media_id    TEXT,              -- NULL if unmatched
    server_internal_id TEXT NOT NULL,    -- Beam's own ID for this item
    title             TEXT NOT NULL,     -- for display of unmatched items
    quality           TEXT,              -- "4K", "1080p", etc.
    deep_link         TEXT NOT NULL,
    playback_link     TEXT NOT NULL,
    external_ids      TEXT,              -- JSON: {tmdb_id, anilist_id, imdb_id}
    last_seen_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (server_id, server_internal_id)
);
CREATE INDEX idx_lib_items_cameo ON server_library_items(cameo_media_id) WHERE cameo_media_id IS NOT NULL;
```

### 6.5 Notification & Job Tables

```sql
-- Notification channels configured by users
CREATE TABLE notification_channels (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_type TEXT NOT NULL CHECK (channel_type IN ('webhook', 'apprise')),
    name        TEXT NOT NULL,
    config      TEXT NOT NULL,  -- JSON: {url, headers} for webhook; {apprise_url, service_urls} for apprise
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Per-user event subscriptions
CREATE TABLE notification_preferences (
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_type    TEXT NOT NULL CHECK (event_type IN (
                      'new_episode', 'available_on_beam',
                      'upcoming_release', 'import_completed'
                  )),
    channel_id    TEXT NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    enabled       INTEGER NOT NULL DEFAULT 1,
    config        TEXT,  -- JSON: event-specific config, e.g., {lead_days: 3} for upcoming_release
    PRIMARY KEY (user_id, event_type, channel_id)
);

-- Background job queue
CREATE TABLE jobs (
    id            TEXT PRIMARY KEY,  -- ULID
    job_type      TEXT NOT NULL,     -- 'beam_sync', 'import', 'notification', etc.
    payload       TEXT,              -- JSON
    status        TEXT NOT NULL DEFAULT 'pending' CHECK (status IN (
                      'pending', 'running', 'completed', 'failed'
                  )),
    attempts      INTEGER NOT NULL DEFAULT 0,
    max_attempts  INTEGER NOT NULL DEFAULT 3,
    run_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    started_at    TEXT,
    completed_at  TEXT,
    error         TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX idx_jobs_pending ON jobs(status, run_at) WHERE status = 'pending';
```

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
| **Auth** | `POST /auth/login`, `POST /auth/refresh`, `POST /auth/logout`, `POST /auth/token` (create API token), `DELETE /auth/token/:id` |
| **Users** | `GET /users/me`, `PATCH /users/me`, `PATCH /users/me/settings`, `GET /users` (admin), `POST /users` (admin), `DELETE /users/:id` (admin) |
| **Media** | `GET /media/search`, `GET /media/:cameo_id`, `GET /media/:cameo_id/seasons`, `GET /media/:cameo_id/seasons/:num/episodes`, `GET /media/:cameo_id/recommendations` |
| **Watch State** | `GET /users/me/media/:cameo_id/status`, `PUT /users/me/media/:cameo_id/status`, `DELETE /users/me/media/:cameo_id/status`, `GET /users/me/episodes/:cameo_episode_id/status`, `PUT /users/me/episodes/:cameo_episode_id/status`, `POST /users/me/media/bulk-status` |
| **Watch History** | `GET /users/me/history` (paginated, filterable), `POST /users/me/history` (manual entry / backfill) |
| **Lists** | `GET /users/me/lists`, `POST /users/me/lists`, `GET /users/me/lists/:id`, `PUT /users/me/lists/:id`, `DELETE /users/me/lists/:id`, `POST /users/me/lists/:id/items`, `DELETE /users/me/lists/:id/items/:cameo_id`, `PUT /users/me/lists/:id/items/reorder` |
| **Tags** | `GET /users/me/tags`, `POST /users/me/tags`, `DELETE /users/me/tags/:id`, `PUT /users/me/media/:cameo_id/tags` |
| **Blocklist** | `GET /users/me/blocklist`, `POST /users/me/blocklist`, `DELETE /users/me/blocklist/:cameo_id` |
| **Discover** | `GET /discover/trending`, `GET /discover/popular`, `GET /discover/upcoming`, `GET /discover/top-rated` (all accept `?media_type=` filter) |
| **Availability** | `GET /media/:cameo_id/availability`, `GET /servers` (list connected streaming servers), `GET /servers/:id/status` |
| **Notifications** | `GET /users/me/notification-channels`, `POST /users/me/notification-channels`, `PUT /users/me/notification-channels/:id`, `DELETE /users/me/notification-channels/:id`, `GET /users/me/notification-preferences`, `PUT /users/me/notification-preferences` |
| **Import/Export** | `POST /users/me/import` (multipart upload or JSON body with OAuth tokens), `GET /users/me/import/:job_id` (status), `GET /users/me/export?format=json\|csv\|mal_xml` |
| **Admin** | `GET /admin/servers`, `POST /admin/servers`, `PUT /admin/servers/:id`, `DELETE /admin/servers/:id`, `POST /admin/servers/:id/sync` (trigger immediate), `GET /admin/jobs`, `POST /admin/jobs/:id/retry` |
| **Stats** | `GET /users/me/stats` (total watch time, genre breakdown, per-year counts, monthly heatmap, media type distribution) |
| **Webhooks** | `POST /webhooks/beam/:server_id` (receives Beam event payloads, validated by webhook secret) |
| **Health** | `GET /health`, `GET /health/ready` |

---

## 8. Deployment

### 8.1 Docker (Single Image)

```yaml
services:
  stacks:
    image: ghcr.io/yourorg/stacks:latest
    container_name: stacks
    ports:
      - "8095:8095"
    environment:
      WW_SECRET_KEY: "generate-a-random-64-char-string"
      WW_TMDB_API_KEY: "your-tmdb-key"           # passed to cameo
      WW_ANILIST_CLIENT_ID: ""                    # optional, for AniList import OAuth
      WW_ANILIST_CLIENT_SECRET: ""                # optional
    volumes:
      - ./data:/data                              # SQLite DB + cameo cache
    restart: unless-stopped
```

The single image contains: the compiled Rust binary, the built React frontend (served by Salvo as static files), and SQLite (linked statically). No runtime dependencies. Target image size: < 50 MB compressed.

### 8.2 Bare Metal

```bash
WW_SECRET_KEY="..." \
WW_TMDB_API_KEY="..." \
  ./stacks serve --bind 0.0.0.0:8095 --data-dir /var/lib/stacks
```

A sample `stacks.service` systemd unit is included in the repo.

### 8.3 Configuration

All configuration via environment variables or a single `config.toml`. Env vars take precedence.

| Variable | Default | Description |
|---|---|---|
| `WW_DATA_DIR` | `/data` | Directory for SQLite DB + cameo cache |
| `WW_SECRET_KEY` | (required) | JWT signing + API key encryption key |
| `WW_BIND` | `0.0.0.0:8095` | Listen address |
| `WW_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `WW_TMDB_API_KEY` | (required) | Passed to cameo for TMDB access |
| `WW_ANILIST_CLIENT_ID` | (empty) | For AniList OAuth import flow |
| `WW_ANILIST_CLIENT_SECRET` | (empty) | For AniList OAuth import flow |
| `WW_BEAM_SYNC_INTERVAL` | `1h` | Fallback poll interval for Beam library sync |
| `WW_OIDC_ISSUER` | (empty) | OIDC provider URL for SSO |
| `WW_OIDC_CLIENT_ID` | (empty) | OIDC client ID |
| `WW_OIDC_CLIENT_SECRET` | (empty) | OIDC client secret |

### 8.4 Backup & Restore

The entire Stacks state is in one directory (`WW_DATA_DIR`):

- `stacks.db` — SQLite database with all user data.
- `cameo/` — cameo's cache directory (can be regenerated from upstream; loss means a cold cache, not data loss).

**Backup:** `stacks backup` CLI subcommand creates a consistent SQLite snapshot (`stacks-backup-YYYYMMDD-HHMMSS.db`). Alternatively, copy the DB file while holding a read lock, or use `sqlite3 .backup`.

**Full export:** `GET /api/v1/users/me/export?format=json` returns a portable JSON archive of all user data. This works even if the SQLite file is corrupted — as long as the API is running, the data is exportable.

**Restore from another instance:** `POST /api/v1/users/me/import` with format `stacks_json` restores all data from a Stacks JSON export.

---

## 9. CLI

```
stacks serve                              # Start the server
stacks migrate                            # Run pending DB migrations
stacks backup [--output <path>]           # Consistent SQLite snapshot
stacks user create                        # Create user interactively
stacks user list                          # List users
stacks user reset-password <username>     # Reset a user's password
stacks import <file> --user <username>    # Import watch data from file
    --format mal_xml|anilist_json|trakt_json|letterboxd_csv|csv|stacks_json
    --conflict skip|overwrite|merge
stacks export --user <username> --format json|csv|mal_xml > export.json
stacks sync --server <id>                 # Trigger immediate Beam sync
stacks health                             # Check server health
stacks config check                       # Validate configuration
```

---

## 10. Observability

- **Structured logging:** JSON to stdout via `tracing` + `tracing-subscriber`. Fields: timestamp, level, span context, request_id.
- **Metrics:** Prometheus-compatible `/metrics` endpoint. Key metrics: API latency histogram (p50/p95/p99), cameo call latency, Beam sync duration, job queue depth, active sessions.
- **Health checks:** `/health` (liveness — process is up) and `/health/ready` (readiness — SQLite writable, cameo responsive, at least one Beam server reachable if configured).

---

## 11. Security

- API keys for Beam servers are encrypted at rest (AES-256-GCM, key derived from `WW_SECRET_KEY`).
- JWT access tokens are short-lived (15 min default). Refresh tokens stored as bcrypt hashes.
- API tokens stored as bcrypt hashes; plaintext shown once at creation.
- CORS configurable (default: same-origin).
- No default credentials. First-run setup creates admin account interactively (CLI or web wizard).
- CSP headers on the frontend. `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`.

---

## 12. Phased Delivery

### Phase 1 — Core (MVP)

- Salvo server with OpenAPI 3.1, SQLite, migrations.
- Cameo integration: search, detail, seasons/episodes, discover feeds.
- User auth (local accounts, JWT, API tokens).
- Watch state: status, score, episode tracking, history.
- Watchlist, favorites, custom lists, blocklist.
- Beam connector: library sync, availability, deep links, playback webhooks.
- Import: MAL XML, TMDB API, Stacks JSON.
- Export: Stacks JSON, CSV.
- Docker single image.
- CLI: `serve`, `migrate`, `user create`, `backup`, `import`, `export`.
- Reference frontend: home, search, library, media detail, watch state controls, Beam availability + play button.

### Phase 2 — Full Feature Set

- Import: AniList (OAuth flow), Trakt JSON, Letterboxd CSV, Simkl, generic CSV.
- Export: MAL-compatible XML.
- Tags, smart lists (saved filters).
- Notification system (webhooks, Apprise).
- Stats dashboard.
- Prometheus `/metrics` endpoint.
- OIDC/OAuth2 SSO.
- Unmatched item resolution UI (admin).
- Frontend: animations and transitions per §5.9 spec.

### Phase 3 — Extensibility

- Additional streaming connectors (Jellyfin, Plex, Emby) — implementing the `StreamingConnector` trait.
- Playback progress sync from multiple sources.
- Import conflict resolution UI (preview diffs before applying).
- Helm chart for Kubernetes.

---

## 13. Open Questions

1. **Beam integration protocol:** Beam's library API shape, external ID exposure, webhook format, and deep-link URL scheme need to be co-designed. The `StreamingConnector` trait defines what Stacks needs; Beam needs to satisfy it.
2. **Cameo episode identifiers:** Are cameo episode IDs stable across cache invalidations? If cameo regenerates IDs after a cache clear, `episode_watch_status` rows would become orphaned. Mitigation: cameo should derive episode IDs deterministically (e.g., `{cameo_media_id}:s{season}e{episode}`) rather than using random IDs.
3. **Cameo library vs. service boundary:** Cameo is described as a library (linked into the Stacks binary). If cameo grows heavy or needs independent scaling, it could become a sidecar service — but for now, in-process is simpler and eliminates network overhead. This should be an explicit design constraint in cameo's API surface.
4. **AniList rate limits for bulk import:** AniList's API allows 90 requests per minute. A user with a 2000-entry anime list would need ~22 requests (90 per page). The import pipeline should respect rate limits with backoff and report progress.
5. **Fractional indexing for list reorder:** Using `REAL` for position avoids reindexing the whole list on every drag-drop, but after many insertions the precision can degrade. Periodic re-normalization (setting positions to 1.0, 2.0, 3.0, ...) should run when the gap between adjacent items drops below a threshold.
