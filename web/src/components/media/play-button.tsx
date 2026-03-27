interface PlayButtonProps {
  playbackLink: string;
  disabled?: boolean;
}

export default function PlayButton({
  playbackLink,
  disabled = false,
}: PlayButtonProps) {
  const handlePlay = () => {
    if (!disabled && playbackLink) {
      // Opens the deep link (e.g. vlc://, infuse://, etc.)
      window.location.href = playbackLink;
    }
  };

  return (
    <button
      type="button"
      data-testid="play-button"
      className="play-button"
      onClick={handlePlay}
      disabled={disabled}
      aria-label="Play"
    >
      {/* Play button stub — opens deep link */}▶ Play
    </button>
  );
}
