import { motion } from "motion/react";

interface PosterCardProps {
  cameoMediaId: string;
  title: string;
  posterUrl?: string;
  watchStatus?: string;
  className?: string;
}

export default function PosterCard({
  cameoMediaId,
  title,
  posterUrl,
  watchStatus,
  className,
}: PosterCardProps) {
  return (
    <motion.div
      data-testid="poster-card"
      data-cameo-media-id={cameoMediaId}
      className={`poster-card ${className ?? ""}`}
      whileHover={{ scale: 1.05 }}
      transition={{ duration: 0.15, ease: "easeOut" as const }}
    >
      {/* Poster card stub — hover: scale 1.05 + shadow + title overlay */}
      {posterUrl ? (
        <img src={posterUrl} alt={title} className="poster-card-image" />
      ) : (
        <div className="poster-card-placeholder" />
      )}
      <div className="poster-card-overlay">
        <span className="poster-card-title">{title}</span>
        {watchStatus && (
          <span className="poster-card-status">{watchStatus}</span>
        )}
      </div>
    </motion.div>
  );
}
