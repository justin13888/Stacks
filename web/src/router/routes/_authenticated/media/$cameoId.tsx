import { createLazyFileRoute } from "@tanstack/react-router";
import MediaDetailHeader from "@/components/media/media-detail-header";
import EpisodeList from "@/components/media/episode-list";
import AvailabilityBadge from "@/components/media/availability-badge";
import PlayButton from "@/components/media/play-button";
import ScoreInput from "@/components/media/score-input";
import StatusSelector from "@/components/media/status-selector";
import { useState } from "react";

export const Route = createLazyFileRoute("/_authenticated/media/$cameoId")({
  component: MediaDetailPage,
});

function MediaDetailPage() {
  const { cameoId } = Route.useParams();
  const [score, setScore] = useState<number | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  return (
    <div data-testid="media-detail-page" className="media-detail-page">
      {/* Media detail page for cameoId: {cameoId} */}
      <MediaDetailHeader title={`Media ${cameoId}`} />
      <AvailabilityBadge available={false} />
      <PlayButton playbackLink="" disabled />
      <StatusSelector value={status} onChange={setStatus} />
      <ScoreInput value={score} onChange={setScore} format="10-point" />
      <EpisodeList cameoMediaId={cameoId} episodes={[]} />
    </div>
  );
}
