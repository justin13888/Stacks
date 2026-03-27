interface MediaDetailHeaderProps {
  backdropUrl?: string;
  posterUrl?: string;
  title: string;
  year?: number;
  genres?: string[];
}

export default function MediaDetailHeader({
  backdropUrl,
  posterUrl,
  title,
  year,
  genres,
}: MediaDetailHeaderProps) {
  return (
    <div data-testid="media-detail-header" className="media-detail-header">
      {/* Full-bleed backdrop with gradient fade */}
      {backdropUrl && (
        <div
          className="media-detail-backdrop"
          style={{ backgroundImage: `url(${backdropUrl})` }}
        />
      )}
      <div className="media-detail-gradient" />
      <div className="media-detail-info">
        {posterUrl && (
          <img src={posterUrl} alt={title} className="media-detail-poster" />
        )}
        <div className="media-detail-meta">
          <h1 className="media-detail-title">{title}</h1>
          {year && <span className="media-detail-year">{year}</span>}
          {genres && genres.length > 0 && (
            <ul className="media-detail-genres">
              {genres.map((g) => (
                <li key={g}>{g}</li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
