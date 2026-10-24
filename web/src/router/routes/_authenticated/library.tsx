import { createFileRoute } from "@tanstack/react-router";
import { useCallback } from "react";
import { LibraryPage } from "@/features/library/library-page";
import {
  type LibrarySearch,
  validateLibrarySearch,
} from "@/features/library/search";

export const Route = createFileRoute("/_authenticated/library")({
  validateSearch: validateLibrarySearch,
  component: LibraryRoute,
});

function LibraryRoute() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const onSearchChange = useCallback(
    (changes: Partial<LibrarySearch>) =>
      void navigate({
        search: (previous) =>
          validateLibrarySearch({ ...previous, ...changes }),
        replace: true,
      }),
    [navigate],
  );
  return <LibraryPage search={search} onSearchChange={onSearchChange} />;
}
