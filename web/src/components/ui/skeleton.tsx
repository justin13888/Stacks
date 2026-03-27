interface SkeletonProps {
  className?: string;
}

export default function Skeleton({ className }: SkeletonProps) {
  return (
    <div
      data-testid="skeleton"
      className={`skeleton-shimmer ${className ?? ""}`}
      aria-hidden="true"
    >
      {/* Shimmer placeholder rectangle */}
    </div>
  );
}
