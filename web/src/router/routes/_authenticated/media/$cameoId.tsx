import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { useApiMediaHandlersGetMedia } from "@/api/media/media";
import AvailabilityBadge from "@/components/media/availability-badge";
import EpisodeList from "@/components/media/episode-list";
import MediaDetailHeader from "@/components/media/media-detail-header";
import PlayButton from "@/components/media/play-button";
import ScoreInput from "@/components/media/score-input";
import StatusSelector from "@/components/media/status-selector";
import { type ApiMedia } from "@/lib/media";

export const Route = createFileRoute("/_authenticated/media/$cameoId")({
  component: MediaDetailPage,
});

function MediaDetailPage() {
  const { cameoId } = Route.useParams();
  const detail = useApiMediaHandlersGetMedia(cameoId);

  // Local UI state — seeded from the server response on first render.
  const [score, setScore] = useState<number | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  // The endpoint returns an `ApiMediaDtoMediaDetailResponse` which is a
  // tuple-shaped wrapper around `unknown` — narrow at the boundary.
  const media = (detail.data as unknown as ApiMedia | undefined) ?? undefined;

  useEffect(() => {
    if (media) {
      if (media.score != null) setScore(media.score);
      if (media.watch_status != null) setStatus(media.watch_status);
    }
  }, [media]);

  if (detail.isLoading) {
    return <div data-testid="media-detail-page">Loading…</div>;
  }

  const playback = media?.availability?.servers?.[0];

  return (
    <div data-testid="media-detail-page" className="media-detail-page">
      <MediaDetailHeader title={media?.title ?? cameoId} />
      <AvailabilityBadge available={!!media?.availability?.available} />
      <PlayButton
        playbackLink={playback?.playback_link ?? ""}
        disabled={!playback}
      />
      <StatusSelector value={status} onChange={setStatus} />
      <ScoreInput value={score} onChange={setScore} format="10-point" />
      <EpisodeList cameoMediaId={cameoId} episodes={[]} />
    </div>
  );
}
