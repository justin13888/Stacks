import { createLazyFileRoute } from "@tanstack/react-router";
import HorizontalRow from "@/components/media/horizontal-row";

export const Route = createLazyFileRoute("/_authenticated/")({
  component: HomePage,
});

// Placeholder data — replace with API data
const PLACEHOLDER_ITEMS = [
  { cameoMediaId: "1", title: "Example Title A", posterUrl: undefined },
  { cameoMediaId: "2", title: "Example Title B", posterUrl: undefined },
  { cameoMediaId: "3", title: "Example Title C", posterUrl: undefined },
];

function HomePage() {
  return (
    <div data-testid="home-page" className="home-page">
      {/* Home page — media shelf rows */}
      <HorizontalRow title="Continue Watching" items={PLACEHOLDER_ITEMS} />
      <HorizontalRow title="Recently Added to Beam" items={PLACEHOLDER_ITEMS} />
      <HorizontalRow title="Trending" items={PLACEHOLDER_ITEMS} />
      <HorizontalRow title="Popular" items={PLACEHOLDER_ITEMS} />
    </div>
  );
}
