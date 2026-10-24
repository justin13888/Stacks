// A deterministic stand-in for TMDB and AniList, for the composed e2e stack.
//
// The API reaches it through WW_TMDB_BASE_URL (`/3/...`) and
// WW_ANILIST_BASE_URL (`/graphql`), so a run never touches the internet and
// always sees the same titles. The outage drills flip each upstream through
// `PUT /__control/{tmdb|anilist}`:
//
//   {"mode": "ok"}                      serve the fixtures (the default)
//   {"mode": "error"}                   answer every call with a 500
//   {"mode": "rate_limited", "retryAfter": 30}
//                                       answer every call with a 429
//
// `GET /__control` reports both modes and how many upstream calls each saw.
// Runs on Bun with no dependencies: `bun run fixtures/server.ts`.

type Mode = "ok" | "error" | "rate_limited";
type Upstream = "tmdb" | "anilist";

interface UpstreamState {
  mode: Mode;
  retryAfter: number;
  calls: number;
}

const state: Record<Upstream, UpstreamState> = {
  tmdb: { mode: "ok", retryAfter: 30, calls: 0 },
  anilist: { mode: "ok", retryAfter: 30, calls: 0 },
};

// --- TMDB fixtures ---

/** The show the journey tracks: one season of four episodes. */
export const SHOW = {
  id: 910001,
  name: "The Lighthouse Keepers",
  overview:
    "Two estranged sisters inherit a lighthouse and the secrets of the town it watches over.",
  first_air_date: "2024-03-01",
  origin_country: ["GB"],
  original_language: "en",
  genres: [{ id: 18, name: "Drama" }],
  episodes: [
    "The Lamp Room",
    "Low Tide",
    "Fog Signal",
    "The Long Night",
  ] as const,
};

/** A film, for search results and feeds. */
export const MOVIE = {
  id: 910002,
  title: "Paper Moons",
  overview: "A projectionist restores a lost silent film, frame by frame.",
  release_date: "2023-11-10",
  original_language: "en",
  genres: [{ id: 18, name: "Drama" }],
  runtime: 104,
};

/** A Korean drama, so the drama feed has something to show. */
export const KDRAMA = {
  id: 910003,
  name: "Seoul Lanterns",
  overview: "A night-market cook and a tired detective share one umbrella.",
  first_air_date: "2025-01-15",
  origin_country: ["KR"],
  original_language: "ko",
  genres: [{ id: 18, name: "Drama" }],
};

/** The anime AniList serves. */
export const ANIME = {
  id: 920001,
  romaji: "Hoshizora no Michi",
  english: "The Starlit Road",
  native: "星空の道",
};

function showSummary(show: typeof SHOW | typeof KDRAMA) {
  return {
    id: show.id,
    name: show.name,
    original_name: show.name,
    overview: show.overview,
    first_air_date: show.first_air_date,
    origin_country: show.origin_country,
    original_language: show.original_language,
    genre_ids: show.genres.map((g) => g.id),
    popularity: 50.0,
    poster_path: null,
    backdrop_path: null,
    vote_average: 8.1,
    vote_count: 120,
    adult: false,
  };
}

function movieSummary() {
  return {
    id: MOVIE.id,
    title: MOVIE.title,
    original_title: MOVIE.title,
    overview: MOVIE.overview,
    release_date: MOVIE.release_date,
    original_language: MOVIE.original_language,
    genre_ids: MOVIE.genres.map((g) => g.id),
    popularity: 40.0,
    poster_path: null,
    backdrop_path: null,
    vote_average: 7.6,
    vote_count: 80,
    adult: false,
    video: false,
  };
}

function page(results: unknown[]) {
  return {
    page: 1,
    total_pages: 1,
    total_results: results.length,
    results,
  };
}

function showDetail(show: typeof SHOW | typeof KDRAMA) {
  const episodes = "episodes" in show ? show.episodes.length : 8;
  return {
    ...showSummary(show),
    genres: show.genres,
    status: "Ended",
    in_production: false,
    number_of_seasons: 1,
    number_of_episodes: episodes,
    episode_run_time: [48],
    last_air_date: show.first_air_date,
    homepage: "",
    tagline: "",
    type: "Scripted",
    seasons: [
      {
        id: show.id * 10 + 1,
        season_number: 1,
        name: "Season 1",
        episode_count: episodes,
        air_date: show.first_air_date,
        overview: "",
        poster_path: null,
      },
    ],
  };
}

function season(show: typeof SHOW) {
  return {
    _id: `season-${show.id}-1`,
    id: show.id * 10 + 1,
    season_number: 1,
    name: "Season 1",
    overview: "",
    air_date: show.first_air_date,
    poster_path: null,
    episodes: show.episodes.map((name, i) => ({
      id: show.id * 100 + i + 1,
      episode_number: i + 1,
      season_number: 1,
      name,
      overview: `Episode ${i + 1} of ${show.name}.`,
      air_date: `2024-03-${String(1 + i * 7).padStart(2, "0")}`,
      runtime: 48,
      still_path: null,
      vote_average: 8.0,
      vote_count: 10,
    })),
  };
}

function movieDetail() {
  return {
    ...movieSummary(),
    genres: MOVIE.genres,
    runtime: MOVIE.runtime,
    status: "Released",
    tagline: "",
    homepage: "",
    imdb_id: null,
    budget: 0,
    revenue: 0,
    belongs_to_collection: null,
    production_companies: [],
    production_countries: [],
    spoken_languages: [],
  };
}

function matches(query: string, ...names: string[]): boolean {
  const q = query.trim().toLowerCase();
  return q === "" || names.some((n) => n.toLowerCase().includes(q));
}

function tmdb(url: URL): Response {
  const path = url.pathname.replace(/^\/3/, "");
  const json = (body: unknown) => Response.json(body);
  const notFound = () =>
    Response.json(
      {
        success: false,
        status_code: 34,
        status_message: "The resource you requested could not be found.",
      },
      { status: 404 },
    );

  if (path === "/search/multi") {
    const q = url.searchParams.get("query") ?? "";
    const results: unknown[] = [];
    if (matches(q, SHOW.name)) {
      results.push({ ...showSummary(SHOW), media_type: "tv" });
    }
    if (matches(q, MOVIE.title)) {
      results.push({ ...movieSummary(), media_type: "movie" });
    }
    if (matches(q, KDRAMA.name)) {
      results.push({ ...showSummary(KDRAMA), media_type: "tv" });
    }
    return json(page(results));
  }
  if (path === `/tv/${SHOW.id}`) return json(showDetail(SHOW));
  if (path === `/tv/${KDRAMA.id}`) return json(showDetail(KDRAMA));
  if (path === `/tv/${SHOW.id}/season/1`) return json(season(SHOW));
  if (path === `/movie/${MOVIE.id}`) return json(movieDetail());
  if (/^\/(tv|movie)\/\d+\/recommendations$/.test(path)) {
    return json(
      page(path.startsWith("/tv") ? [showSummary(KDRAMA)] : [movieSummary()]),
    );
  }
  if (path === "/discover/tv") return json(page([showSummary(KDRAMA)]));
  if (/^\/(trending\/tv\/week|tv\/popular|tv\/top_rated)$/.test(path)) {
    return json(page([showSummary(SHOW), showSummary(KDRAMA)]));
  }
  if (
    /^\/(trending\/movie\/week|movie\/popular|movie\/top_rated)$/.test(path)
  ) {
    return json(page([movieSummary()]));
  }
  return notFound();
}

// --- AniList fixtures ---

function anilistMedia() {
  return {
    id: ANIME.id,
    idMal: 930001,
    title: {
      romaji: ANIME.romaji,
      english: ANIME.english,
      native: ANIME.native,
    },
    synonyms: [],
    description: "A courier walks the length of a road made of starlight.",
    startDate: { year: 2024, month: 4, day: 6 },
    endDate: { year: 2024, month: 6, day: 22 },
    coverImage: null,
    bannerImage: null,
    genres: ["Adventure", "Fantasy"],
    popularity: 1000,
    averageScore: 84,
    episodes: 12,
    duration: 24,
    status: "FINISHED",
    format: "TV",
    countryOfOrigin: "JP",
    isAdult: false,
    season: "SPRING",
    seasonYear: 2024,
    studios: { nodes: [{ name: "Studio Fixture" }] },
  };
}

async function anilist(req: Request): Promise<Response> {
  const body = (await req.json().catch(() => ({}))) as {
    query?: string;
    variables?: Record<string, unknown>;
  };
  const query = body.query ?? "";
  if (/\bPage\s*\(/.test(query)) {
    const vars = body.variables ?? {};
    const search = String(vars.query ?? "");
    // List queries filter by format; the fixture anime is a TV series.
    const formats = Array.isArray(vars.formatIn) ? vars.formatIn : ["TV"];
    const media =
      formats.includes("TV") && matches(search, ANIME.romaji, ANIME.english)
        ? [anilistMedia()]
        : [];
    return Response.json({
      data: {
        Page: {
          pageInfo: {
            total: media.length,
            currentPage: 1,
            lastPage: 1,
            hasNextPage: false,
          },
          media,
          staff: [],
        },
      },
    });
  }
  if (/\bMedia\s*\(/.test(query)) {
    const vars = body.variables ?? {};
    const known = vars.id === ANIME.id || vars.idMal === 930001;
    if (!known) {
      return Response.json(
        {
          data: { Media: null },
          errors: [{ message: "Not Found.", status: 404 }],
        },
        { status: 404 },
      );
    }
    return Response.json({ data: { Media: anilistMedia() } });
  }
  return Response.json(
    { data: null, errors: [{ message: "Unsupported query.", status: 400 }] },
    { status: 400 },
  );
}

// --- Fault injection ---

function fault(upstream: Upstream): Response | null {
  const s = state[upstream];
  if (s.mode === "error") {
    return Response.json(
      { status_message: "Internal error (fixture outage)" },
      { status: 500 },
    );
  }
  if (s.mode === "rate_limited") {
    return Response.json(
      {
        data: null,
        errors: [{ message: "Too Many Requests.", status: 429 }],
      },
      { status: 429, headers: { "Retry-After": String(s.retryAfter) } },
    );
  }
  return null;
}

async function control(req: Request, url: URL): Promise<Response> {
  if (req.method === "GET" && url.pathname === "/__control") {
    return Response.json(state);
  }
  const upstream = url.pathname.slice("/__control/".length);
  if (req.method !== "PUT" || (upstream !== "tmdb" && upstream !== "anilist")) {
    return new Response("not found", { status: 404 });
  }
  const body = (await req.json().catch(() => ({}))) as {
    mode?: string;
    retryAfter?: number;
  };
  if (
    body.mode !== "ok" &&
    body.mode !== "error" &&
    body.mode !== "rate_limited"
  ) {
    return new Response("mode must be ok, error or rate_limited", {
      status: 400,
    });
  }
  state[upstream].mode = body.mode;
  if (typeof body.retryAfter === "number") {
    state[upstream].retryAfter = body.retryAfter;
  }
  return Response.json(state[upstream]);
}

const port = Number(process.env.PORT ?? 8090);

Bun.serve({
  port,
  hostname: "0.0.0.0",
  async fetch(req) {
    const url = new URL(req.url);
    if (url.pathname === "/__health") return new Response("ok");
    if (url.pathname.startsWith("/__control")) return control(req, url);
    if (url.pathname.startsWith("/3/")) {
      state.tmdb.calls += 1;
      return fault("tmdb") ?? tmdb(url);
    }
    if (url.pathname === "/graphql" && req.method === "POST") {
      state.anilist.calls += 1;
      return fault("anilist") ?? anilist(req);
    }
    return new Response("not found", { status: 404 });
  },
});

console.log(`metadata fixtures listening on :${port}`);
