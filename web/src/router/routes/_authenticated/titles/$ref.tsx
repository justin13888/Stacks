import { createFileRoute } from "@tanstack/react-router";
import { TitlePage } from "@/features/titles/title-page";

export const Route = createFileRoute("/_authenticated/titles/$ref")({
  component: TitleRoute,
});

function TitleRoute() {
  const { ref } = Route.useParams();
  // A new title is a new page: nothing typed or chosen carries over.
  return <TitlePage key={ref} titleRef={ref} />;
}
