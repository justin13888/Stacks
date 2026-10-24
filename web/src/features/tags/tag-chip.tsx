import type { ButtonHTMLAttributes } from "react";
import type { Tag } from "@/api/schemas";
import { cn } from "@/lib/cn";

/** A tag's colour swatch; tags without one get the neutral line colour. */
export function TagSwatch({ color }: { color?: string | null }) {
  return (
    <span
      aria-hidden
      className="size-2.5 shrink-0 rounded-full ring-1 ring-line-strong ring-inset"
      style={{ backgroundColor: color ?? "var(--color-line)" }}
    />
  );
}

/** A toggleable tag chip, e.g. a Library filter. */
export function TagChip({
  tag,
  pressed,
  className,
  ...props
}: { tag: Pick<Tag, "name" | "color">; pressed: boolean } & Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "children"
>) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      className={cn(
        "inline-flex h-8 items-center gap-1.5 rounded-full border border-line bg-surface px-3 text-sm text-fg-muted",
        "transition-colors duration-(--duration-fast) hover:text-fg focus-visible:outline-2 focus-visible:outline-ring",
        pressed && "border-accent bg-accent-soft text-fg",
        className,
      )}
      {...props}
    >
      <TagSwatch color={tag.color} />
      {tag.name}
    </button>
  );
}
