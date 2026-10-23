/**
 * Response shapes of the Salvo-era API (before /api/v1 moved to kynos, #6).
 *
 * These pages were written against that API through `authFetch` and still
 * read its shapes. The new API's typed client is the orval output in
 * `src/api/`, generated from `web/openapi.json`; the pages move onto it as
 * the web app is rebuilt (#8–#11), and this file goes with them. Until then
 * the pages compile against the shapes they were written for, and do not
 * work against the new API.
 */

export interface ApiMediaDtoAvailabilityServer {
  server_id: string;
  server_name: string;
  quality?: string | null;
  deep_link: string;
  playback_link: string;
}

export interface ApiMediaDtoAvailability {
  available: boolean;
  servers: ApiMediaDtoAvailabilityServer[];
}

export interface ApiMediaDtoMediaItem {
  cameo_media_id: string;
  title: string;
  media_type: string;
  year?: number | null;
  overview?: string | null;
  poster_url?: string | null;
  backdrop_url?: string | null;
  genres: string[];
  rating?: number | null;
  watch_status?: string | null;
  score?: number | null;
  is_favorite: boolean;
  availability: ApiMediaDtoAvailability;
}

export interface ApiMediaDtoMediaSearchResponse {
  items: ApiMediaDtoMediaItem[];
  next_cursor?: string | null;
}

export interface ApiMediaDtoMediaDetailResponse {
  media: ApiMediaDtoMediaItem;
  tagline?: string | null;
  runtime_minutes?: number | null;
  status?: string | null;
  homepage?: string | null;
  imdb_id?: string | null;
  number_of_seasons?: number | null;
  number_of_episodes?: number | null;
  in_production?: boolean | null;
}

export interface ApiMediaDtoSeason {
  season_number: number;
  name?: string | null;
  overview?: string | null;
  air_date?: string | null;
  poster_url?: string | null;
  episode_count: number;
}

export interface ApiMediaDtoSeasonsResponse {
  seasons: ApiMediaDtoSeason[];
}

export interface ApiMediaDtoEpisode {
  season_number: number;
  episode_number: number;
  name?: string | null;
  overview?: string | null;
  air_date?: string | null;
  runtime_minutes?: number | null;
  still_url?: string | null;
  rating?: number | null;
}

export interface ApiMediaDtoEpisodesResponse {
  episodes: ApiMediaDtoEpisode[];
}

export interface ApiDiscoverDtoDiscoverResponse {
  items: ApiMediaDtoMediaItem[];
  next_cursor?: string | null;
}

export interface ApiWatchStateDtoLibraryResponse {
  items: ApiMediaDtoMediaItem[];
  next_cursor?: string | null;
}

export interface ApiWatchStateDtoWatchStatusResponse {
  cameo_media_id: string;
  status: string;
  score?: number | null;
  notes?: string | null;
  is_favorite: boolean;
  started_at?: string | null;
  completed_at?: string | null;
  updated_at: string;
}

export interface ApiListsDtoListResponse {
  id: string;
  name: string;
  description?: string | null;
  is_smart: boolean;
  item_count: number;
  created_at: string;
  updated_at: string;
}

export interface ApiImportExportDtoImportStatusResponse {
  job_id: string;
  status: string;
  attempts: number;
  error?: string | null;
}

export interface ApiStatsDtoGenreCount {
  genre: string;
  count: number;
}

export interface ApiStatsDtoYearCount {
  year: number;
  count: number;
}

export interface ApiStatsDtoMonthCount {
  month: string;
  count: number;
}

export interface ApiStatsDtoStatsResponse {
  total_watch_time_minutes: number;
  genre_breakdown: ApiStatsDtoGenreCount[];
  yearly_counts: ApiStatsDtoYearCount[];
  monthly_heatmap: ApiStatsDtoMonthCount[];
}

export interface ApiUserDtoUserResponse {
  id: string;
  username: string;
  email: string;
  role: string;
  settings: unknown;
  created_at: string;
  updated_at: string;
}

export interface ApiNotificationsDtoChannelResponse {
  id: string;
  channel_type: string;
  name: string;
  enabled: boolean;
  created_at: string;
}

export interface ApiNotificationsDtoNotificationPreference {
  event_type: string;
  channel_id: string;
  enabled: boolean;
  config?: unknown;
}

export interface ApiNotificationsDtoPreferencesResponse {
  preferences: ApiNotificationsDtoNotificationPreference[];
}
