import { Link } from "@tanstack/react-router";
import { Compass } from "lucide-react";
import type { ReactNode } from "react";
import { EmptyState } from "@/components/ui/empty-state";

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div className="flex flex-col gap-1">
        <h1 className="font-display text-2xl font-bold tracking-tight text-fg md:text-3xl">
          {title}
        </h1>
        {description && <p className="text-sm text-fg-muted">{description}</p>}
      </div>
      {actions}
    </header>
  );
}

/** An unknown address, or a section this account cannot use. */
export function PageNotFound() {
  return (
    <EmptyState
      icon={<Compass />}
      title="Nothing here"
      description="This page doesn't exist, or it moved."
      action={
        <Link
          to="/"
          className="text-sm font-medium text-accent hover:underline"
        >
          Go home
        </Link>
      }
    />
  );
}
