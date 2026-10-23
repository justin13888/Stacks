import { Clapperboard } from "lucide-react";
import { useState } from "react";
import { cn } from "@/lib/cn";

export type PosterSize = "xs" | "sm" | "md" | "lg";

const widths: Record<PosterSize, string> = {
  xs: "w-10",
  sm: "w-24",
  md: "w-36",
  lg: "w-56",
};

export interface PosterProps {
  /** The poster image, or `null` when the provider has none. */
  src: string | null | undefined;
  /** The title the poster belongs to; it is the image's alt text. */
  title: string;
  size?: PosterSize;
  className?: string;
  /** Load eagerly (above the fold) instead of lazily. */
  priority?: boolean;
}

/** Shades derived from the title, so each fallback poster looks distinct. */
function fallbackHue(title: string): number {
  let hash = 0;
  for (const char of title) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return Math.abs(hash) % 360;
}

/**
 * A title's poster, locked to the 2:3 poster ratio so a grid never reflows.
 * It fades in over a blurred placeholder once loaded, and falls back to
 * generated art when there is no image or it fails to load.
 */
export function Poster({
  src,
  title,
  size = "md",
  className,
  priority = false,
}: PosterProps) {
  const [state, setState] = useState<"loading" | "loaded" | "failed">(
    "loading",
  );
  // A new image starts loading again (adjusted during render).
  const [seenSrc, setSeenSrc] = useState(src);
  if (seenSrc !== src) {
    setSeenSrc(src);
    setState("loading");
  }

  const showFallback = !src || state === "failed";
  const hue = fallbackHue(title);

  return (
    <div
      data-state={showFallback ? "fallback" : state}
      className={cn(
        "relative aspect-[2/3] shrink-0 overflow-hidden rounded-poster bg-surface-sunken shadow-poster",
        widths[size],
        className,
      )}
    >
      {showFallback ? (
        <div
          role="img"
          aria-label={title}
          className="flex size-full flex-col items-center justify-center gap-2 p-2 text-center"
          style={{
            background: `linear-gradient(160deg, oklch(45% 0.12 ${hue}), oklch(25% 0.08 ${(hue + 40) % 360}))`,
          }}
        >
          <Clapperboard aria-hidden className="size-1/4 text-white/70" />
          {size !== "xs" && (
            <span className="line-clamp-3 text-xs font-semibold text-white/90">
              {title}
            </span>
          )}
        </div>
      ) : (
        <>
          <div
            aria-hidden
            className={cn(
              "absolute inset-0 animate-shimmer bg-[linear-gradient(90deg,var(--color-surface-sunken)_0%,var(--color-surface-raised)_50%,var(--color-surface-sunken)_100%)] bg-size-[200%_100%]",
              "transition-opacity duration-(--duration-slow)",
              state === "loaded" && "opacity-0",
            )}
          />
          <img
            src={src}
            alt={title}
            loading={priority ? "eager" : "lazy"}
            decoding="async"
            onLoad={() => setState("loaded")}
            onError={() => setState("failed")}
            className={cn(
              "absolute inset-0 size-full object-cover transition-[opacity,filter] duration-(--duration-slow) ease-(--ease-out-soft)",
              state === "loaded" ? "opacity-100 blur-0" : "opacity-0 blur-md",
            )}
          />
        </>
      )}
    </div>
  );
}
