import { createFileRoute } from "@tanstack/react-router";
import { PagePlaceholder } from "@/components/layout/page";

export const Route = createFileRoute("/_authenticated/titles/$ref")({
  component: TitlePage,
});

function TitlePage() {
  const { ref } = Route.useParams();
  return (
    <PagePlaceholder
      title="Title"
      description={`Seasons, episodes, your progress and score for ${ref}.`}
    />
  );
}
