import { createFileRoute } from "@tanstack/react-router";
import {
  ImportPage,
  validateImportSearch,
} from "@/features/import/import-page";

export const Route = createFileRoute("/_authenticated/settings/import")({
  validateSearch: validateImportSearch,
  component: ImportRoute,
});

function ImportRoute() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  return (
    <ImportPage
      importId={search.import}
      onOpenImport={(id) => void navigate({ search: id ? { import: id } : {} })}
    />
  );
}
