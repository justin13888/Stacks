import { useCallback, useState } from "react";

export interface MediaFilters {
  mediaType: string[];
  genre: string[];
  yearMin: number | null;
  yearMax: number | null;
  status: string[];
  availableOnBeam: boolean | null;
  scoreMin: number | null;
  scoreMax: number | null;
  sort: string;
}

const DEFAULT_FILTERS: MediaFilters = {
  mediaType: [],
  genre: [],
  yearMin: null,
  yearMax: null,
  status: [],
  availableOnBeam: null,
  scoreMin: null,
  scoreMax: null,
  sort: "title_asc",
};

export function useMediaFilters(initial?: Partial<MediaFilters>) {
  const [filters, setFilters] = useState<MediaFilters>({
    ...DEFAULT_FILTERS,
    ...initial,
  });

  const updateFilter = useCallback(
    <K extends keyof MediaFilters>(key: K, value: MediaFilters[K]) => {
      setFilters((prev) => ({ ...prev, [key]: value }));
    },
    [],
  );

  const resetFilters = useCallback(() => {
    setFilters(DEFAULT_FILTERS);
  }, []);

  const hasActiveFilters =
    filters.mediaType.length > 0 ||
    filters.genre.length > 0 ||
    filters.status.length > 0 ||
    filters.yearMin !== null ||
    filters.yearMax !== null ||
    filters.availableOnBeam !== null;

  return { filters, updateFilter, resetFilters, hasActiveFilters };
}
