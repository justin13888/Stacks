/**
 * Helpers that adapt API-shape data (snake_case, nullable fields) into the
 * camelCase shapes that the existing presentational components expect.
 */
import type { ApiMediaDtoMediaItem } from "@/api/schemas";

export interface DisplayMediaItem {
  cameoMediaId: string;
  title: string;
  posterUrl?: string;
  watchStatus?: string;
}

/** Convert an `ApiMediaDtoMediaItem` into the shape used by PosterGrid/HorizontalRow. */
export function toDisplayItem(item: ApiMediaDtoMediaItem): DisplayMediaItem {
  return {
    cameoMediaId: item.cameo_media_id,
    title: item.title,
    posterUrl: item.poster_url ?? undefined,
    watchStatus: item.watch_status ?? undefined,
  };
}

export function toDisplayItems(
  items: ApiMediaDtoMediaItem[],
): DisplayMediaItem[] {
  return items.map(toDisplayItem);
}
