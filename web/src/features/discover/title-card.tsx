import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { EyeOff, ListPlus, MoreHorizontal, Plus, Star } from "lucide-react";
import { useState } from "react";
import type { TitleSummary } from "@/api/schemas";
import { CATEGORY_NAMES } from "@/components/search/categories";
import { IconButton } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { reportFailure } from "@/components/ui/error-state";
import { Poster } from "@/components/ui/poster";
import { StatusBadge } from "@/components/ui/status-badge";
import { toast } from "@/components/ui/toast";
import { useNotInterested } from "@/features/blocklist/use-blocklist";
import { addToLibrary, invalidateTracking } from "@/features/library/entry";
import { AddToListDialog } from "@/features/lists/add-to-list";
import { displayTitle, usePreferences } from "@/features/titles/preferences";
import { cn } from "@/lib/cn";

/** Adds a title to Planning from a card. */
function useAddToPlanning() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (title: TitleSummary) => addToLibrary(title.ref, "planning"),
    onSuccess: (outcome, title) => {
      toast(
        outcome === "kept"
          ? { title: "Already in your library", description: title.title }
          : {
              tone: "success",
              title: "Added to Planning",
              description: title.title,
            },
      );
    },
    onError: reportFailure,
    // Refetches the listings too: the cards' badges come from them.
    onSettled: (_outcome, _error, title) =>
      invalidateTracking(queryClient, title.ref),
  });
}

/**
 * A catalog title as a poster card: its library status when it is in the
 * library, otherwise a quick "add to Planning"; and a menu to add it to a
 * list or hide it ("Not interested").
 */
export function TitleCard({
  title,
  size = "md",
}: {
  title: TitleSummary;
  /** `sm` for the narrow rails on the title page. */
  size?: "sm" | "md";
}) {
  const { title_language } = usePreferences();
  const name = displayTitle(title, title_language);
  const add = useAddToPlanning();
  const hide = useNotInterested();
  const [listOpen, setListOpen] = useState(false);
  const facts = [title.year, CATEGORY_NAMES[title.category]]
    .filter(Boolean)
    .join(" · ");

  return (
    <article
      aria-label={name}
      className={cn(
        "group relative flex flex-col gap-1.5",
        size === "sm" ? "w-28" : "w-36",
      )}
    >
      <Link
        to="/titles/$ref"
        params={{ ref: title.ref }}
        className="flex flex-col gap-1.5 rounded-poster focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      >
        <Poster
          src={title.poster_url}
          title={name}
          size={size}
          className="w-full transition-transform duration-(--duration-base) group-hover:-translate-y-0.5"
        />
        <span className="line-clamp-2 text-sm font-medium text-fg">{name}</span>
      </Link>
      <span className="flex items-center gap-1 text-xs text-fg-muted">
        {title.rating != null && (
          <>
            <Star aria-hidden className="size-3 fill-warning text-warning" />
            <span>
              <span className="sr-only">Rated </span>
              {title.rating.toFixed(1)}
            </span>
            <span aria-hidden>·</span>
          </>
        )}
        <span className="truncate">{facts}</span>
      </span>
      <div className="flex items-center justify-between gap-1">
        {title.library ? (
          <StatusBadge
            status={title.library.status}
            rewatching={title.library.rewatching}
          />
        ) : (
          <IconButton
            label={`Add ${name} to Planning`}
            variant="secondary"
            size="sm"
            icon={<Plus aria-hidden className="size-4" />}
            disabled={add.isPending}
            aria-busy={add.isPending || undefined}
            onClick={() => add.mutate(title)}
          />
        )}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <IconButton
              label={`More for ${name}`}
              size="sm"
              icon={<MoreHorizontal aria-hidden className="size-4" />}
            />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={() => setListOpen(true)}>
              <ListPlus aria-hidden className="size-4" />
              Add to list…
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => hide.mutate(title)}>
              <EyeOff aria-hidden className="size-4" />
              Not interested
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <AddToListDialog
        title={title}
        open={listOpen}
        onOpenChange={setListOpen}
      />
    </article>
  );
}
