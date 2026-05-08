/**
 * Loose runtime shape of a media-with-user-state row coming back from the
 * API. The OpenAPI surface types these as `unknown` because the Rust DTOs
 * use `serde_json::Value`. We re-narrow them at the page boundary so each
 * component receives the strict prop type it expects.
 */
export interface ApiMedia {
  cameo_media_id: string;
  title?: string;
  media_type?: string;
  year?: number | null;
  overview?: string | null;
  poster_url?: string | null;
  backdrop_url?: string | null;
  genres?: string[];
  rating?: number | null;
  watch_status?: string | null;
  score?: number | null;
  is_favorite?: boolean;
  availability?: {
    available: boolean;
    servers: Array<{
      server_id: string;
      server_name: string;
      quality?: string | null;
      deep_link: string;
      playback_link: string;
    }>;
  };
}

export function toCardItem(item: unknown) {
  const m = item as ApiMedia;
  return {
    cameoMediaId: m.cameo_media_id,
    title: m.title ?? "(untitled)",
    posterUrl: m.poster_url ?? undefined,
    watchStatus: m.watch_status ?? undefined,
  };
}
