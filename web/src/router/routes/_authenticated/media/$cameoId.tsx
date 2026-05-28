import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import type {
  ApiMediaDtoEpisodesResponse,
  ApiMediaDtoMediaDetailResponse,
  ApiMediaDtoSeasonsResponse,
  ApiWatchStateDtoWatchStatusResponse,
} from "@/api/schemas";
import AvailabilityBadge from "@/components/media/availability-badge";
import EpisodeList from "@/components/media/episode-list";
import MediaDetailHeader from "@/components/media/media-detail-header";
import PlayButton from "@/components/media/play-button";
import ScoreInput from "@/components/media/score-input";
import StatusSelector from "@/components/media/status-selector";
import { authFetch } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/media/$cameoId")({
  component: MediaDetailPage,
});

interface StatusUpdate {
  status: string;
  score?: number | null;
  is_favorite?: boolean;
  notes?: string;
  started_at?: string;
  completed_at?: string;
}

function MediaDetailPage() {
  const { cameoId } = Route.useParams();
  const queryClient = useQueryClient();
  const [selectedSeason, setSelectedSeason] = useState<number | null>(null);

  const detailKey = ["media-detail", cameoId];
  const seasonsKey = ["media-seasons", cameoId];
  const statusKey = ["media-status", cameoId];

  const detail = useQuery({
    queryKey: detailKey,
    queryFn: ({ signal }) =>
      authFetch<ApiMediaDtoMediaDetailResponse>({
        url: `/api/v1/media/${cameoId}`,
        method: "GET",
        signal,
      }),
  });

  const isTv = detail.data?.media.media_type === "tv";

  const seasons = useQuery({
    queryKey: seasonsKey,
    enabled: isTv,
    queryFn: ({ signal }) =>
      authFetch<ApiMediaDtoSeasonsResponse>({
        url: `/api/v1/media/${cameoId}/seasons`,
        method: "GET",
        signal,
      }),
  });

  const effectiveSeason =
    selectedSeason ?? seasons.data?.seasons[0]?.season_number ?? null;

  const episodes = useQuery({
    queryKey: ["media-episodes", cameoId, effectiveSeason],
    enabled: isTv && effectiveSeason !== null,
    queryFn: ({ signal }) =>
      authFetch<ApiMediaDtoEpisodesResponse>({
        url: `/api/v1/media/${cameoId}/seasons/${effectiveSeason}/episodes`,
        method: "GET",
        signal,
      }),
  });

  const status = useQuery({
    queryKey: statusKey,
    retry: false,
    queryFn: ({ signal }) =>
      authFetch<ApiWatchStateDtoWatchStatusResponse | null>({
        url: `/api/v1/users/me/media/${cameoId}/status`,
        method: "GET",
        signal,
      }).catch(() => null),
  });

  const statusMutation = useMutation({
    mutationFn: (update: StatusUpdate) =>
      authFetch<ApiWatchStateDtoWatchStatusResponse>({
        url: `/api/v1/users/me/media/${cameoId}/status`,
        method: "PUT",
        data: update,
      }),
    onSuccess: (next) => {
      queryClient.setQueryData(statusKey, next);
      queryClient.invalidateQueries({ queryKey: detailKey });
    },
  });

  const handleStatusChange = (next: string) => {
    if (!next) return;
    statusMutation.mutate({
      status: next,
      score: status.data?.score ?? null,
      is_favorite: status.data?.is_favorite ?? false,
    });
  };

  const handleScoreChange = (next: number) => {
    statusMutation.mutate({
      status: status.data?.status ?? "watching",
      score: Number.isFinite(next) ? next : null,
      is_favorite: status.data?.is_favorite ?? false,
    });
  };

  if (detail.isLoading) {
    return (
      <div data-testid="media-detail-page" className="media-detail-page">
        <p className="media-detail-loading">Loading…</p>
      </div>
    );
  }

  if (detail.isError || !detail.data) {
    return (
      <div data-testid="media-detail-page" className="media-detail-page">
        <p className="media-detail-error">Couldn't load this title.</p>
      </div>
    );
  }

  const media = detail.data.media;
  const availability = media.availability;
  const primaryServer = availability.servers[0];

  const episodeList = (episodes.data?.episodes ?? []).map((ep) => ({
    id: `${effectiveSeason}-${ep.episode_number}`,
    number: ep.episode_number,
    title: ep.name ?? `Episode ${ep.episode_number}`,
    watched: false, // per-episode watched state needs the episodes/status endpoint
  }));

  return (
    <div data-testid="media-detail-page" className="media-detail-page">
      <MediaDetailHeader
        title={media.title}
        year={media.year ?? undefined}
        genres={media.genres}
        posterUrl={media.poster_url ?? undefined}
        backdropUrl={media.backdrop_url ?? undefined}
      />

      <div className="media-detail-actions">
        <AvailabilityBadge
          available={availability.available}
          serverName={primaryServer?.server_name}
        />
        <PlayButton
          playbackLink={primaryServer?.playback_link ?? ""}
          disabled={!primaryServer}
        />
        <StatusSelector
          value={status.data?.status ?? null}
          onChange={handleStatusChange}
        />
        <ScoreInput
          value={status.data?.score ?? null}
          onChange={handleScoreChange}
          format="10-point"
        />
      </div>

      {isTv && (
        <section className="media-detail-seasons">
          <h2 className="media-detail-seasons-title">Seasons</h2>
          {seasons.isLoading ? (
            <p>Loading seasons…</p>
          ) : (
            <div className="media-detail-seasons-tabs">
              {(seasons.data?.seasons ?? []).map((season) => {
                const isActive = effectiveSeason === season.season_number;
                return (
                  <button
                    type="button"
                    key={season.season_number}
                    className={`media-detail-season-tab ${isActive ? "media-detail-season-tab--active" : ""}`}
                    onClick={() => setSelectedSeason(season.season_number)}
                  >
                    {season.name ?? `Season ${season.season_number}`}
                  </button>
                );
              })}
            </div>
          )}

          {effectiveSeason !== null &&
            (episodes.isLoading ? (
              <p>Loading episodes…</p>
            ) : (
              <EpisodeList
                cameoMediaId={cameoId}
                episodes={episodeList}
                onMarkUpTo={() => {
                  /* per-episode mark requires episode_id endpoint */
                }}
              />
            ))}
        </section>
      )}
    </div>
  );
}
