import { createLazyFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import SearchBar from "@/components/search/search-bar";
import SearchResults from "@/components/search/search-results";

export const Route = createLazyFileRoute("/_authenticated/search")({
  component: SearchPage,
});

function SearchPage() {
  const [query, setQuery] = useState("");

  const handleSubmit = () => {
    // TODO: trigger search
  };

  return (
    <div data-testid="search-page" className="search-page">
      {/* Search page */}
      <SearchBar value={query} onChange={setQuery} onSubmit={handleSubmit} />
      <SearchResults results={[]} loading={false} />
    </div>
  );
}
