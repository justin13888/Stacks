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

/**
 * The tags a title carries, read-only: a swatch and a name each. Nothing
 * when it carries none.
 */
export function TagList({
  tags,
  label,
  className,
}: {
  tags: readonly Pick<Tag, "id" | "name" | "color">[];
  label: string;
  className?: string;
}) {
  if (tags.length === 0) return null;
  return (
    <ul
      aria-label={label}
      className={cn("flex min-w-0 flex-wrap gap-x-2 gap-y-1", className)}
    >
      {tags.map((tag) => (
        <li
          key={tag.id}
          className="inline-flex min-w-0 items-center gap-1 text-xs text-fg-muted"
        >
          <TagSwatch color={tag.color} />
          <span className="truncate">{tag.name}</span>
        </li>
      ))}
    </ul>
  );
}

/** A toggleable tag chip, e.g. a Library filter. */
export function TagChip({
  tag,
  pressed,
  className,
  ...props
}: {
  tag: Pick<Tag, "name" | "color">;
  /** `"mixed"` when only some of what it applies to carry the tag. */
  pressed: boolean | "mixed";
} & Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children">) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      className={cn(
        "inline-flex h-8 items-center gap-1.5 rounded-full border border-line bg-surface px-3 text-sm text-fg-muted",
        "transition-colors duration-(--duration-fast) hover:text-fg focus-visible:outline-2 focus-visible:outline-ring",
        pressed === true && "border-accent bg-accent-soft text-fg",
        pressed === "mixed" && "border-dashed border-accent text-fg",
        "disabled:opacity-60",
        className,
      )}
      {...props}
    >
      <TagSwatch color={tag.color} />
      {tag.name}
    </button>
  );
}
