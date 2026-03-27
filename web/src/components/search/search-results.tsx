import { motion } from "motion/react";
import { staggeredFadeIn } from "@/lib/animation";
import Spinner from "@/components/ui/spinner";

interface SearchResultItem {
  cameoMediaId: string;
  title: string;
  posterUrl?: string;
}

interface SearchResultsProps {
  results: SearchResultItem[];
  loading?: boolean;
}

export default function SearchResults({
  results,
  loading = false,
}: SearchResultsProps) {
  if (loading) {
    return (
      <div data-testid="search-results" className="search-results">
        <Spinner />
      </div>
    );
  }

  return (
    <div data-testid="search-results" className="search-results">
      {/* Search results — staggered fade-in */}
      {results.map((item, index) => (
        <motion.div
          key={item.cameoMediaId}
          className="search-result-item"
          data-cameo-media-id={item.cameoMediaId}
          variants={staggeredFadeIn(index)}
          initial="initial"
          animate="animate"
        >
          {item.posterUrl && (
            <img
              src={item.posterUrl}
              alt={item.title}
              className="search-result-thumbnail"
            />
          )}
          <span className="search-result-title">{item.title}</span>
        </motion.div>
      ))}
    </div>
  );
}
