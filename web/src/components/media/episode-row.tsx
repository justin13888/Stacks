import { motion } from "motion/react";

interface Episode {
  id: string;
  number: number;
  title: string;
  watched: boolean;
}

interface EpisodeRowProps {
  episode: Episode;
  onToggle: (id: string, watched: boolean) => void;
}

export default function EpisodeRow({ episode, onToggle }: EpisodeRowProps) {
  return (
    <div
      data-testid="episode-row"
      data-episode-id={episode.id}
      className={`episode-row ${episode.watched ? "episode-row--watched" : ""}`}
    >
      {/* Episode row stub — checkbox with scale pulse on toggle */}
      <motion.label
        className="episode-row-checkbox-label"
        whileTap={{ scale: 1.2 }}
        transition={{ duration: 0.15 }}
      >
        <input
          type="checkbox"
          checked={episode.watched}
          onChange={(e) => onToggle(episode.id, e.target.checked)}
        />
      </motion.label>
      <span className="episode-row-number">{episode.number}</span>
      <span className="episode-row-title">{episode.title}</span>
    </div>
  );
}
