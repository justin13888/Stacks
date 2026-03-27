import EpisodeRow from "./episode-row";

interface Episode {
  id: string;
  number: number;
  title: string;
  watched: boolean;
}

interface EpisodeListProps {
  cameoMediaId: string;
  episodes: Episode[];
  onMarkUpTo?: (episodeId: string) => void;
}

export default function EpisodeList({
  cameoMediaId,
  episodes,
  onMarkUpTo,
}: EpisodeListProps) {
  const handleToggle = (id: string, watched: boolean) => {
    if (!watched && onMarkUpTo) {
      onMarkUpTo(id);
    }
  };

  return (
    <div
      data-testid="episode-list"
      data-cameo-media-id={cameoMediaId}
      className="episode-list"
    >
      {/* Episode list stub */}
      {episodes.length === 0 ? (
        <p className="episode-list-empty">No episodes found.</p>
      ) : (
        episodes.map((ep) => (
          <EpisodeRow key={ep.id} episode={ep} onToggle={handleToggle} />
        ))
      )}
    </div>
  );
}
